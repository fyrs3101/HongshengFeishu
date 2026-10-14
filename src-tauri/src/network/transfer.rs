// TCP 传输层：文本消息（回执/加密）+ 文件/文件夹（zip 打包）分片传输 + 接收确认
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use tauri::{Emitter, Manager};
use zip::write::FileOptions;
use zip::ZipWriter;

use crate::network::discovery;
use crate::network::protocol::*;
use crate::state::{AppState, InnerState, PendingOffer};

const CHUNK_SIZE: usize = 48 * 1024; // base64 编码前的原始分片大小

/// 启动 TCP 监听（系统分配端口，端口随广播通告）
pub fn start_listener(app: tauri::AppHandle, state: Arc<Mutex<InnerState>>) {
    thread::spawn(move || {
        let listener = match TcpListener::bind(("0.0.0.0", 0)) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[transfer] 监听失败: {}", e);
                return;
            }
        };
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        state.lock().unwrap().self_port = port;
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

// ---- 入站处理 ----

fn handle_incoming(stream: &mut TcpStream, app: tauri::AppHandle, state: Arc<Mutex<InnerState>>) {
    let sender_addr = stream.peer_addr().ok();
    // transfer_id -> (文件, 总大小, 已接收)
    let mut active: HashMap<String, (File, u64, u64)> = HashMap::new();
    let mut rejected_local: std::collections::HashSet<String> = std::collections::HashSet::new();

    while let Some(frame) = read_frame(stream) {
        match frame {
            Frame::Text {
                from,
                nick,
                msg_id,
                text,
                cipher,
            } => {
                let enc = cipher.is_some();
                let display = cipher.unwrap_or(text);
                let msg = ChatMessage::text_in(&from, &display, enc);
                upsert_message(&state, &msg);
                let _ = app.emit("message", msg);
                notify_incoming(&app, &state, &nick, &display);
                // 回执：告诉对方消息已送达
                let peer_port = state.lock().unwrap().peers.get(&from).map(|p| (p.ip.clone(), p.port));
                let target = match peer_port {
                    Some((ip, port)) => Some((ip, port)),
                    None => sender_addr.map(|a| (a.ip().to_string(), a.port())),
                };
                if let Some((ip, port)) = target {
                    let _ = thread::spawn(move || send_ack(&ip, port, Some(msg_id), String::new()));
                }
            }
            Frame::FileMeta {
                from,
                nick,
                transfer_id,
                name,
                size,
                is_dir,
            } => {
                let (auto_accept, inbox) = {
                    let s = state.lock().unwrap();
                    (s.settings.auto_accept, s.inbox_dir.clone())
                };

                let decided: Option<Option<String>> = if auto_accept {
                    Some(None)
                } else {
                    // 弹窗等待用户确认（120 秒超时按拒绝处理）
                    let (tx, rx) = channel();
                    {
                        let mut s = state.lock().unwrap();
                        s.pending_offers.insert(
                            transfer_id.clone(),
                            PendingOffer {
                                from: from.clone(),
                                nick: nick.clone(),
                                transfer_id: transfer_id.clone(),
                                name: name.clone(),
                                size,
                                is_dir,
                                resp: tx,
                            },
                        );
                    }
                    let _ = app.emit(
                        "file-offer",
                        serde_json::json!({
                            "from": from, "nick": nick, "transfer_id": transfer_id,
                            "name": name, "size": size, "is_dir": is_dir,
                        }),
                    );
                    rx.recv_timeout(Duration::from_secs(120)).ok()
                };

                match decided {
                    None => {
                        // 拒绝或超时：告知发送方并记录一条已拒绝的消息
                        rejected_local.insert(transfer_id.clone());
                        let msg = ChatMessage {
                            id: gen_uid(),
                            peer_uid: from.clone(),
                            direction: "in".into(),
                            kind: "file".into(),
                            text: String::new(),
                            enc: false,
                            file_name: name.clone(),
                            file_size: size,
                            is_dir,
                            transfer_id: transfer_id.clone(),
                            status: "rejected".into(),
                            progress: 0,
                            speed_bps: 0,
                            eta_secs: 0,
                            saved_path: String::new(),
                            ts: now_ms(),
                        };
                        upsert_message(&state, &msg);
                        let _ = app.emit("message", msg);
                        if let Some(peer) = state.lock().unwrap().peers.get(&from).cloned() {
                            let tid = transfer_id.clone();
                            let _ = thread::spawn(move || send_reject(&peer.ip, peer.port, &tid));
                        }
                    }
                    Some(save_hint) => {
                        let path = resolve_save_path(&inbox, &name, save_hint.as_deref());
                        if let Some(parent) = Path::new(&path).parent() {
                            let _ = fs::create_dir_all(parent);
                        }
                        let mut msg = ChatMessage {
                            id: gen_uid(),
                            peer_uid: from.clone(),
                            direction: "in".into(),
                            kind: "file".into(),
                            text: String::new(),
                            enc: false,
                            file_name: name.clone(),
                            file_size: size,
                            is_dir,
                            transfer_id: transfer_id.clone(),
                            status: "receiving".into(),
                            progress: 0,
                            speed_bps: 0,
                            eta_secs: 0,
                            saved_path: path.clone(),
                            ts: now_ms(),
                        };
                        match fs::File::create(&path) {
                            Ok(f) => {
                                active.insert(transfer_id.clone(), (f, size, 0));
                                upsert_message(&state, &msg);
                                let _ = app.emit("message", msg);
                            }
                            Err(_) => {
                                msg.status = "failed".into();
                                upsert_message(&state, &msg);
                                let _ = app.emit("message", msg);
                            }
                        }
                    }
                }
            }
            Frame::FileChunk {
                transfer_id, data, ..
            } => {
                if rejected_local.contains(&transfer_id) {
                    continue; // 已拒绝：丢弃分片
                }
                if let Some((file, size, received)) = active.get_mut(&transfer_id) {
                    if let Ok(bytes) = B64.decode(&data) {
                        if file.write_all(&bytes).is_ok() {
                            *received += bytes.len() as u64;
                            let progress = if *size > 0 {
                                (*received * 100 / *size).min(100)
                            } else {
                                100
                            };
                            update_progress(&state, &app, &transfer_id, progress, "receiving", 0, 0);
                        }
                    }
                }
            }
            Frame::FileDone { transfer_id } => {
                active.remove(&transfer_id);
                update_progress(&state, &app, &transfer_id, 100, "received", 0, 0);
                if let Some(name) = find_file_name(&state, &transfer_id) {
                    notify_incoming(&app, &state, "", &format!("文件已接收：{}", name));
                }
            }
            Frame::Ack { msg_id, .. } => {
                if let Some(mid) = msg_id {
                    mark_delivered(&state, &app, &mid);
                }
            }
            Frame::Reject { transfer_id } => {
                state.lock().unwrap().rejected.insert(transfer_id.clone());
                update_progress(&state, &app, &transfer_id, 0, "rejected", 0, 0);
            }
        }
    }
}

fn find_file_name(state: &Arc<Mutex<InnerState>>, transfer_id: &str) -> Option<String> {
    let s = state.lock().unwrap();
    for list in s.messages.values() {
        if let Some(m) = list.iter().find(|m| m.transfer_id == transfer_id) {
            return Some(m.file_name.clone());
        }
    }
    None
}

/// 保存路径：用户指定优先（目录则拼接文件名，完整路径则直接用），否则收入件箱并去重
fn resolve_save_path(inbox: &str, name: &str, hint: Option<&str>) -> String {
    match hint {
        Some(h) if !h.trim().is_empty() => {
            let p = Path::new(h);
            if p.is_dir() || h.ends_with('/') || h.ends_with('\\') {
                unique_path(h, name)
            } else {
                if let Some(parent) = p.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                h.to_string()
            }
        }
        _ => unique_path(inbox, name),
    }
}

// ---- 发送侧 ----

/// 发送文本；pass 为 Some 时按口令加密
pub fn send_text(
    app: tauri::AppHandle,
    state: Arc<Mutex<InnerState>>,
    peer_uid: String,
    text: String,
    pass: Option<String>,
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
        (s.self_uid.clone(), s.settings.nick.clone())
    };
    // 先单播 hello，确保隐身方/新联系人也知道我们的端口
    discovery::unicast_hello(&state, &peer.ip);

    let enc = matches!(&pass, Some(p) if !p.is_empty());
    let msg = ChatMessage::text_out(&peer_uid, &text, enc);
    let msg_id = msg.id.clone();
    let (frame_text, cipher) = match &pass {
        Some(p) if !p.is_empty() => (String::new(), Some(encrypt_text(&text, p))),
        _ => (text.clone(), None),
    };

    let mut stream =
        TcpStream::connect((peer.ip.as_str(), peer.port)).map_err(|e| format!("连接失败: {}", e))?;
    write_frame(
        &mut stream,
        &Frame::Text {
            from: self_uid,
            nick: self_nick,
            msg_id,
            text: frame_text,
            cipher,
        },
    )
    .map_err(|e| format!("发送失败: {}", e))?;
    let _ = stream.shutdown(std::net::Shutdown::Both);

    upsert_message(&state, &msg);
    let _ = app.emit("message", msg.clone());
    Ok(msg.id)
}

/// 发送文件或文件夹（文件夹先打包 zip）
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
    let (self_uid, self_nick) = {
        let s = state.lock().unwrap();
        (s.self_uid.clone(), s.settings.nick.clone())
    };
    discovery::unicast_hello(&state, &peer.ip);

    let meta = fs::metadata(&path).map_err(|e| format!("文件读取失败: {}", e))?;
    let is_dir = meta.is_dir();

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

    let size = fs::metadata(&send_path)
        .map_err(|e| format!("读取失败: {}", e))?
        .len();
    let transfer_id = gen_uid();

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
    let mut last_tick = std::time::Instant::now();
    let mut last_bytes: u64 = 0;

    let msg = ChatMessage {
        id: gen_uid(),
        peer_uid: peer_uid.clone(),
        direction: "out".into(),
        kind: "file".into(),
        text: String::new(),
        enc: false,
        file_name: send_name.clone(),
        file_size: size,
        is_dir,
        transfer_id: transfer_id.clone(),
        status: "sending".into(),
        progress: 0,
        speed_bps: 0,
        eta_secs: 0,
        saved_path: send_path.clone(),
        ts: now_ms(),
    };
    upsert_message(&state, &msg);
    let _ = app.emit("message", msg);

    loop {
        // 对方拒绝 → 中止
        if state.lock().unwrap().rejected.contains(&transfer_id) {
            state.lock().unwrap().rejected.remove(&transfer_id);
            update_progress(&state, &app, &transfer_id, 0, "rejected", 0, 0);
            if is_dir {
                let _ = fs::remove_file(&send_path);
            }
            return Err("对方拒绝了接收该文件".to_string());
        }
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

        // 每 300ms 刷新一次速度与进度
        let dt = last_tick.elapsed().as_secs_f64();
        if dt >= 0.3 {
            let speed = ((sent - last_bytes) as f64 / dt) as u64;
            let eta = if speed > 0 && sent < size {
                (size - sent) / speed
            } else {
                0
            };
            let progress = if size > 0 { (sent * 100 / size).min(100) } else { 100 };
            update_progress(&state, &app, &transfer_id, progress, "sending", speed, eta);
            last_tick = std::time::Instant::now();
            last_bytes = sent;
        }
    }
    write_frame(
        &mut stream,
        &Frame::FileDone {
            transfer_id: transfer_id.clone(),
        },
    )
    .map_err(|e| format!("结束帧失败: {}", e))?;
    let _ = stream.shutdown(std::net::Shutdown::Both);

    update_progress(&state, &app, &transfer_id, 100, "sent", 0, 0);
    if is_dir {
        let _ = fs::remove_file(&send_path);
    }
    Ok(transfer_id)
}

// ---- 移动端分块发送（JS 读取文件后分块推入）----

/// 开始一个分块文件传输，返回 transfer_id（落盘到临时暂存区）
pub fn begin_file_bytes(state: tauri::State<AppState>, name: String) -> Result<String, String> {
    let transfer_id = gen_uid();
    let tmp = std::env::temp_dir().join(format!("hsfs-up-{}-{}", transfer_id, sanitize(&name)));
    fs::File::create(&tmp).map_err(|e| format!("创建暂存文件失败: {}", e))?;
    state
        .0
        .lock()
        .unwrap()
        .staging
        .insert(transfer_id.clone(), tmp);
    Ok(transfer_id)
}

/// 追加一个 base64 分片
pub fn push_file_chunk(
    state: tauri::State<AppState>,
    transfer_id: String,
    data: String,
) -> Result<(), String> {
    let path = {
        let s = state.0.lock().unwrap();
        s.staging.get(&transfer_id).cloned()
    }
    .ok_or_else(|| "传输不存在".to_string())?;
    let bytes = B64.decode(&data).map_err(|e| format!("分片解码失败: {}", e))?;
    let mut f = fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .map_err(|e| format!("写入暂存失败: {}", e))?;
    f.write_all(&bytes).map_err(|e| format!("写入暂存失败: {}", e))
}

/// 结束分块传输并发送
pub fn finish_file_bytes(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    transfer_id: String,
    peer_uid: String,
) -> Result<String, String> {
    let path = {
        let mut s = state.0.lock().unwrap();
        s.staging.remove(&transfer_id)
    }
    .ok_or_else(|| "传输不存在".to_string())?;
    let path = path.to_string_lossy().to_string();
    send_file(app, state.0.clone(), peer_uid, path)
}

// ---- 小工具 ----

fn send_ack(ip: &str, port: u16, msg_id: Option<String>, transfer_id: String) {
    if let Ok(mut s) = TcpStream::connect((ip, port)) {
        let _ = write_frame(&mut s, &Frame::Ack { msg_id, transfer_id });
        let _ = s.shutdown(std::net::Shutdown::Both);
    }
}

fn send_reject(ip: &str, port: u16, transfer_id: &str) {
    if let Ok(mut s) = TcpStream::connect((ip, port)) {
        let _ = write_frame(
            &mut s,
            &Frame::Reject {
                transfer_id: transfer_id.to_string(),
            },
        );
        let _ = s.shutdown(std::net::Shutdown::Both);
    }
}

fn upsert_message(state: &Arc<Mutex<InnerState>>, msg: &ChatMessage) {
    let mut s = state.lock().unwrap();
    let list = s.messages.entry(msg.peer_uid.clone()).or_insert_with(Vec::new);
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
    speed_bps: u64,
    eta_secs: u64,
) {
    let found = {
        let mut s = state.lock().unwrap();
        let mut hit: Option<ChatMessage> = None;
        for list in s.messages.values_mut() {
            if let Some(m) = list.iter_mut().find(|m| m.transfer_id == transfer_id) {
                m.progress = progress;
                m.status = status.to_string();
                if speed_bps > 0 {
                    m.speed_bps = speed_bps;
                }
                m.eta_secs = eta_secs;
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

fn mark_delivered(state: &Arc<Mutex<InnerState>>, app: &tauri::AppHandle, msg_id: &str) {
    let found = {
        let mut s = state.lock().unwrap();
        let mut hit: Option<ChatMessage> = None;
        for list in s.messages.values_mut() {
            if let Some(m) = list.iter_mut().find(|m| m.id == msg_id) {
                if m.status == "sending" {
                    m.status = "delivered".into();
                    hit = Some(m.clone());
                }
                break;
            }
        }
        hit
    };
    if let Some(msg) = found {
        let _ = app.emit("message", msg);
    }
}

/// 收到消息时：应用不在前台则发系统通知
fn notify_incoming(app: &tauri::AppHandle, state: &Arc<Mutex<InnerState>>, nick: &str, body: &str) {
    use tauri_plugin_notification::NotificationExt;
    // 已知窗口标签：主窗口 + 设置/传输 + 各聊天窗口
    let mut labels = vec![
        "main".to_string(),
        "settings".to_string(),
        "transfers".to_string(),
    ];
    labels.extend(state.lock().unwrap().chat_windows.values().cloned());
    let any_focused = labels.iter().any(|l| {
        app.get_webview_window(l)
            .map(|w| w.is_focused().unwrap_or(false))
            .unwrap_or(false)
    });
    if any_focused {
        return;
    }
    let title = if nick.is_empty() {
        "鸿升飞书".to_string()
    } else {
        format!("鸿升飞书 · {}", nick)
    };
    let preview: String = body.chars().take(60).collect();
    let _ = state;
    let _ = app
        .notification()
        .builder()
        .title(&title)
        .body(&preview)
        .show();
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c == '/' || c == '\\' || c == ':' { '_' } else { c })
        .collect()
}

fn unique_path(dir: &str, name: &str) -> String {
    let base = Path::new(dir).join(sanitize(name));
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

/// 在资源管理器/访达中显示文件或目录
pub fn open_path(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err("文件不存在".to_string());
    }
    let is_dir = p.is_dir();
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        let result = if is_dir {
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open").arg(path).status()
            }
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("explorer").arg(path).status()
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("xdg-open").arg(path).status()
            }
        } else {
            #[cfg(target_os = "macos")]
            {
                std::process::Command::new("open").arg("-R").arg(path).status()
            }
            #[cfg(target_os = "windows")]
            {
                std::process::Command::new("explorer")
                    .arg(format!("/select,{}", path))
                    .status()
            }
            #[cfg(target_os = "linux")]
            {
                std::process::Command::new("xdg-open")
                    .arg(p.parent().unwrap_or(p))
                    .status()
            }
        };
        result.map(|_| ()).map_err(|e| format!("打开失败: {}", e))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = (p, is_dir);
        Ok(()) // 安卓等平台由界面展示路径
    }
}

#[allow(dead_code)]
fn _unused(_p: PathBuf) {}
