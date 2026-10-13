// 鸿升飞书 · Tauri 应用入口与命令
mod network;
mod state;

use std::path::PathBuf;

use tauri::{Emitter, Manager};

use network::discovery;
use network::protocol::*;
use network::transfer;
use state::AppState;

/// 启动：初始化自身信息、收件箱，并拉起发现与监听线程
#[tauri::command]
fn start(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    nick: Option<String>,
) -> Result<(), String> {
    {
        let mut s = state.0.lock().unwrap();
        if s.running {
            return Ok(());
        }
        if let Some(n) = nick {
            s.self_nick = n;
        }
        if s.self_nick.trim().is_empty() {
            s.self_nick = "鸿升周老师".to_string();
        }
        if s.self_uid.is_empty() {
            s.self_uid = gen_uid();
        }
        s.self_port = TRANSFER_PORT;
        let dir = app
            .path()
            .app_data_dir()
            .ok()
            .map(|p| p.join("inbox"))
            .unwrap_or_else(|| PathBuf::from("inbox"));
        let _ = std::fs::create_dir_all(&dir);
        s.inbox_dir = dir.to_string_lossy().to_string();
        s.running = true;
    }
    let st = state.0.clone();
    discovery::start_discovery(app.clone(), st.clone());
    transfer::start_listener(app.clone(), st.clone());
    Ok(())
}

/// 停止：下线广播并清空联系人
#[tauri::command]
fn stop(app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    {
        let mut s = state.0.lock().unwrap();
        s.running = false;
        s.peers.clear();
    }
    discovery::broadcast_bye(&state.0.clone());
    let _ = app.emit("peers", Vec::<Peer>::new());
    Ok(())
}

#[tauri::command]
fn list_peers(state: tauri::State<AppState>) -> Vec<Peer> {
    let s = state.0.lock().unwrap();
    s.peers.values().cloned().collect()
}

#[tauri::command]
fn get_self(state: tauri::State<AppState>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    serde_json::json!({ "uid": s.self_uid, "nick": s.self_nick, "port": s.self_port })
}

#[tauri::command]
fn set_nick(state: tauri::State<AppState>, nick: String) -> Result<(), String> {
    let mut s = state.0.lock().unwrap();
    if nick.trim().is_empty() {
        return Err("昵称不能为空".into());
    }
    s.self_nick = nick;
    Ok(())
}

#[tauri::command]
fn send_text(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_uid: String,
    text: String,
) -> Result<String, String> {
    transfer::send_text(app, state.0.clone(), peer_uid, text)
}

#[tauri::command]
fn send_file(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_uid: String,
    path: String,
) -> Result<String, String> {
    transfer::send_file(app, state.0.clone(), peer_uid, path)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            start, stop, list_peers, get_self, set_nick, send_text, send_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
