// 聊天记录持久化：messages.json（应用配置目录）
// 采用脏标记 + 3 秒节流写入，避免文件分片进度高频刷新导致频繁落盘
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use crate::network::protocol::ChatMessage;
use crate::state::{lock_state, InnerState};
use tauri::Manager;

/// 每个会话保留的最大消息条数（超出丢弃最旧）
const MAX_PER_PEER: usize = 1000;

fn messages_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .map(|d: PathBuf| d.join("messages.json"))
        .unwrap_or_else(|_| PathBuf::from("messages.json"))
}

/// 加载历史消息（文件不存在或损坏时返回空表）
pub fn load_messages(app: &tauri::AppHandle) -> HashMap<String, Vec<ChatMessage>> {
    let path = messages_path(app);
    let Ok(text) = fs::read_to_string(&path) else {
        return HashMap::new();
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 全量写入（截断超长会话）
pub fn save_messages(app: &tauri::AppHandle, state: &Arc<Mutex<InnerState>>) {
    let path = messages_path(app);
    let snapshot: HashMap<String, Vec<ChatMessage>> = {
        let s = lock_state(&state);
        s.messages
            .iter()
            .map(|(k, v)| {
                let start = v.len().saturating_sub(MAX_PER_PEER);
                (k.clone(), v[start..].to_vec())
            })
            .collect()
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // 先写临时文件再原子替换，避免写一半损坏
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_string(&snapshot) {
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

/// 启动后台持久化线程：每 3 秒检查脏标记并落盘；退出前保存一次
pub fn spawn_persister(app: tauri::AppHandle, state: Arc<Mutex<InnerState>>) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(3));
            let (dirty, running) = {
                let mut s = lock_state(&state);
                let d = s.messages_dirty;
                s.messages_dirty = false;
                (d, s.running)
            };
            if dirty {
                save_messages(&app, &state);
            }
            if !running {
                save_messages(&app, &state);
                break;
            }
        }
    });
}
