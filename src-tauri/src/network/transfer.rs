// TCP 传输层：文本消息 + 文件/文件夹（目录先打包为 zip）分片传输
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use tauri::Emitter;
use zip::write::FileOptions;
use zip::ZipWriter;

use crate::network::protocol::*;
use crate::state::InnerState;

const CHUNK_SIZE: usize = 48 * 1024; // base64 编码前的原始分片大小

/// 启动 TCP 监听（传输端口）
pub fn start_listener(app: tauri::AppHandle, state: Arc<Mutex<InnerState>>) {
    thread::spawn(move || {
        let port = state.lock().unwrap().self_port;
        let listener = match TcpListener::bind(("0.0.0.0", port)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[transfer] 监听端口 {} 失败: {}", port, e);
                return;
            }
        };
        for stream in listener.incoming() {
            if !state.lock().unwrap().running {
                break;
            }
            if let Ok(mut s) = stream {
                let app2 = app.clone();
                let st2 = state.clone();
                thread::spawn(move || handle_incoming(&mut s, app2, st2));
            }
        }
    });
}

/// 处理单个入站连接（一条连接对应一条消息或一个文件传输）
fn handle_incoming(stream: &mut TcpStream, app: tauri::AppHandle, state: Arc<Mutex<InnerState>>) {
    let mut active: HashMap<String, (File, u64, u64, String)> = HashMap::new();
    while let Some(frame) = read_frame(stream) {
        match frame {
            Frame::Text { from, nick, text } => {
                let msg = ChatMessage::text_in(&from, &nick, &text);
                upsert_message(&state, &msg);
                let _ = app.emit("message", msg);
            }
            Frame::FileMeta {
                from,
                nick,
                transfer_id,
                name,
                size,
                is_dir,
            } => {
                let _ = nick;
                let inbox = state.lock().unwrap().inbox_dir.clone();
                let safe = sanitize(&name);
                let path = unique_path(&inbox, &safe);
                if let Some(parent) = Path::new(&path).parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let file = fs::File::create(&path).ok();
                let mut msg = ChatMessage {
                    id: gen_uid(),
                    peer_uid: from.clone(),
                    direction: "in".into(),
                    kind: "file".into(),
                    text: String::new(),
                    file_name: name.clone(),
                    file_size: size,
                    is_dir,
                    transfer_id: transfer_id.clone(),
                    status: "receiving".into(),
                    progress: 0,
                    ts: now_ms(),
                };
                match file {
                    Some(f) => {
                        active.insert(transfer_id.clone(), (f, size, 0, path));
                        upsert_message(&state, &msg);
                        let _ = app.emit("message", msg);
                    }
                    None => {
                        msg.status = "failed".into();
                        upsert_message(&state, &msg);
                        let _ = app.emit("message", msg);
                    }
                }
            }
            Frame::FileChunk {
                transfer_id,
                index: _,
                total: _,
                data,
            } => {
                if let Some((file, size, received, _path)) = active.get_mut(&transfer_id) {
                    if let Ok(bytes) = B64.decode(&data) {
                        let _ = file.write_all(&bytes);
                        let new_recv = *received + bytes.len() as u64;
                        *received = new_recv;
                        let progress = if *size > 0 {
                            (new_recv * 100 / *size)
                        } else {
                            100
                        };
                        update_progress(&state, &app, &transfer_id, progress, "receiving");
                    }
                }
            }
            Frame::FileDone { transfer_id } => {
                if let Some((_f, _size, _received, _path)) = active.remove(&transfer_id) {
                    update_progress(&state, &app, &transfer_id, 100, "received");
                }
            }
            Frame::Ack { .. } => {}
        }
    }
}

// ---- 发送侧 ----

pub fn send_text(
    app: tauri::AppHandle,
    state: Arc<Mutex<InnerState>>,
    peer_uid: String,
    text: String,
) -> Result<String, String> {
    let peer = state
        .lock()
        .unwrap()
        .peers
        .get(&peer_uid)
        .cloned()
        .ok_or_else(|| "对方不在线".to_string())?;
    let (self_uid, self_nick) = {
        let s = state.lock().unwrap();
        (s.self_uid.clone(), s.self_nick.clone())
    };
    let mut stream =
        TcpStream::connect((peer.ip.as_str(), peer.port)).map_err(|e| format!("连接失败: {}", e))?;
    let frame = Frame::Text {
        from: self_uid,
        nick: self_nick,
        text: text.clone(),
    };
    write_frame(&mut stream, &frame).map_err(|e| format!("发送失败: {}", e))?;
    let _ = stream.shutdown(std::net::Shutdown::Both);

    let msg = ChatMessage::text_out(&peer_uid, &text);
    upsert_message(&state, &msg);
    let _ = app.emit("message", msg.clone());
    Ok(msg.id)
}

pub fn send_file(
    app: tauri::AppHandle,
    state: Arc<Mutex<InnerState>>,
    peer_uid: String,
    path: String,
) -> Result<String, String> {
    let peer = state
        .lock()
        .unwrap()
        .peers
        .get(&peer_uid)
        .cloned()
        .ok_or_else(|| "对方不在线".to_string())?;
    let meta =
        fs::metadata(&path).map_err(|e| format!("文件读取失败: {}", e))?;
    let is_dir = meta.is_dir();

    // 文件夹先打包为临时 zip
    let (send_path, send_name) = if is_dir {
        let base = Path::new(&path);
        let folder_name = base
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "folder".to_string());
        let tmp = std::env::temp_dir().join(format!("{}-{}.zip", folder_name, gen_uid()));
        zip_dir(base, &tmp).map_err(|e| format!("打包文件夹失败: {}", e))?;
        (tmp.to_string_lossy().to_string(), format!("{}.zip", folder_name))
    } else {
        (
            path.clone(),
            Path::new(&path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "file".to_string()),
        )
    };

    let size = fs::metadata(&send_path).map_err(|e| format!("读取失败: {}", e))?.len();
    let transfer_id = gen_uid();
    let (self_uid, self_nick) = {
        let s = state.lock().unwrap();
        (s.self_uid.clone(), s.self_nick.clone())
    };

    let mut stream = TcpStream::connect((peer.ip.as_str(), peer.port))
        .map_err(|e| format!("连接失败: {}", e))?;
    write_frame(
        &mut stream,
        &Frame::FileMeta {
            from: self_uid.clone(),
            nick: self_nick.clone(),
            transfer_id: transfer_id.clone(),
            name: send_name.clone(),
            size,
            is_dir,
        },
    )
    .map_err(|e| format!("发送失败: {}", e))?;

    let mut file = fs::File::open(&send_path).map_err(|e| format!("打开失败: {}", e))?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut index: u64 = 0;
    let mut sent: u64 = 0;

    let msg = ChatMessage {
        id: gen_uid(),
        peer_uid: peer_uid.clone(),
        direction: "out".into(),
        kind: "file".into(),
        text: String::new(),
        file_name: send_name.clone(),
        file_size: size,
        is_dir,
        transfer_id: transfer_id.clone(),
        status: "sending".into(),
        progress: 0,
        ts: now_ms(),
    };
    upsert_message(&state, &msg);
    let _ = app.emit("message", msg.clone());

    loop {
        let n = file.read(&mut buf).map_err(|e| format!("读取失败: {}", e))?;
        if n == 0 {
            break;
        }
        let chunk = B64.encode(&buf[..n]);
        write_frame(
            &mut stream,
            &Frame::FileChunk {
                transfer_id: transfer_id.clone(),
                index,
                total: 0,
                data: chunk,
            },
        )
        .map_err(|e| format!("分片发送失败: {}", e))?;
        index += 1;
        sent += n as u64;
        let progress = if size > 0 { sent * 100 / size } else { 100 };
        update_progress(&state, &app, &transfer_id, progress, "sending");
    }
    write_frame(
        &mut stream,
        &Frame::FileDone {
            transfer_id: transfer_id.clone(),
        },
    )
    .map_err(|e| format!("结束帧失败: {}", e))?;
    let _ = stream.shutdown(std::net::Shutdown::Both);

    update_progress(&state, &app, &transfer_id, 100, "sent");
    if is_dir {
        let _ = fs::remove_file(&send_path);
    }
    Ok(transfer_id)
}

// ---- 工具 ----

fn upsert_message(state: &Arc<Mutex<InnerState>>, msg: &ChatMessage) {
    let mut s = state.lock().unwrap();
    let list = s
        .messages
        .entry(msg.peer_uid.clone())
        .or_insert_with(Vec::new);
    if let Some(existing) = list.iter_mut().find(|m| m.id == msg.id) {
        *existing = msg.clone();
    } else {
        list.push(msg.clone());
    }
}

fn update_progress(
    state: &Arc<Mutex<InnerState>>,
    app: &tauri::AppHandle,
    transfer_id: &str,
    progress: u64,
    status: &str,
) {
    let found = {
        let mut s = state.lock().unwrap();
        let mut hit: Option<ChatMessage> = None;
        for list in s.messages.values_mut() {
            if let Some(m) = list.iter_mut().find(|m| m.transfer_id == transfer_id) {
                m.progress = progress;
                m.status = status.to_string();
                hit = Some(m.clone());
                break;
            }
        }
        hit
    };
    if let Some(msg) = found {
        let _ = app.emit("message", msg);
    }
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c == '/' || c == '\\' || c == ':' { '_' } else { c })
        .collect()
}

fn unique_path(dir: &str, name: &str) -> String {
    let base = Path::new(dir).join(name);
    if !base.exists() {
        return base.to_string_lossy().to_string();
    }
    let stem = Path::new(name)
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "file".to_string());
    let ext = Path::new(name)
        .extension()
        .map(|s| format!(".{}", s.to_string_lossy()))
        .unwrap_or_default();
    let mut i = 1;
    loop {
        let candidate = Path::new(dir).join(format!("{} ({}){}", stem, i, ext));
        if !candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
        i += 1;
    }
}

/// 把目录递归打包为 zip
fn zip_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    let file = fs::File::create(dst)?;
    let mut zw = ZipWriter::new(file);
    add_dir_to_zip(&mut zw, src, src)?;
    zw.finish()?;
    Ok(())
}

fn add_dir_to_zip(zw: &mut ZipWriter<File>, base: &Path, dir: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(base).unwrap_or(&path);
        let rel_name = rel.to_string_lossy().to_string();
        let options = FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        if path.is_dir() {
            zw.add_directory(rel_name, options)?;
            add_dir_to_zip(zw, base, &path)?;
        } else {
            zw.start_file(rel_name, options)?;
            let mut f = fs::File::open(&path)?;
            let mut buf = Vec::new();
            f.read_to_end(&mut buf)?;
            zw.write_all(&buf)?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
fn _unused(_p: PathBuf) {}
