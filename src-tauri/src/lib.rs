// 鸿升飞书 · Tauri 应用入口与命令
mod network;
mod persist;
mod state;

use std::fs;
use std::path::{Path, PathBuf};
#[cfg(desktop)]
use std::sync::OnceLock;

#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem};
#[cfg(desktop)]
use tauri::tray::TrayIconBuilder;
use tauri::{Emitter, Manager, WindowEvent};

use network::discovery;
use network::protocol::*;
use network::transfer;
use state::{AppState, Settings};

/// 全局托盘图标（桌面端，用于刷新提示文本）
#[cfg(desktop)]
static TRAY_ICON: OnceLock<tauri::tray::TrayIcon> = OnceLock::new();

/// 更新托盘提示：鸿升飞书 · N 人在线
#[cfg(desktop)]
pub fn update_tray_tooltip(_app: &tauri::AppHandle, online: usize) {
    if let Some(t) = TRAY_ICON.get() {
        let _ = t.set_tooltip(Some(&format!("鸿升飞书 · {} 人在线", online)));
    }
}

// ---- 设置持久化 ----

fn settings_path(app: &tauri::AppHandle) -> PathBuf {
    app.path()
        .app_config_dir()
        .map(|d| d.join("settings.json"))
        .unwrap_or_else(|_| PathBuf::from("settings.json"))
}

fn load_settings(app: &tauri::AppHandle) -> Settings {
    let path = settings_path(app);
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn persist_settings(app: &tauri::AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app);
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| format!("保存设置失败: {}", e))
}

/// 开机自启动（仅桌面端有效）
#[allow(unused_variables)]
fn apply_autostart(app: &tauri::AppHandle, enable: bool) {
    #[cfg(desktop)]
    {
        use tauri_plugin_autostart::ManagerExt;
        let al = app.autolaunch();
        if enable {
            let _ = al.enable();
        } else {
            let _ = al.disable();
        }
    }
}

// ---- 生命周期 ----

/// 启动：加载设置、初始化自身信息与收件箱，拉起发现与监听线程
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
        let mut settings = load_settings(&app);
        if let Some(n) = nick {
            if !n.trim().is_empty() {
                settings.nick = n.trim().to_string();
            }
        }
        if settings.nick.trim().is_empty() {
            settings.nick = "鸿升周老师".to_string();
        }
        s.settings = settings;
        if s.self_uid.is_empty() {
            s.self_uid = gen_uid();
        }
        // 载入历史聊天记录
        let history = persist::load_messages(&app);
        s.messages = history;
    }
    let inbox = {
        let s = state.0.lock().unwrap();
        s.settings
            .inbox_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                app.path()
                    .app_data_dir()
                    .map(|d| d.join("inbox"))
                    .unwrap_or_else(|_| PathBuf::from("inbox"))
            })
    };
    let _ = fs::create_dir_all(&inbox);
    {
        let mut s = state.0.lock().unwrap();
        s.inbox_dir = inbox.to_string_lossy().to_string();
        s.running = true;
    }
    discovery::start_discovery(app.clone(), state.0.clone());
    transfer::start_listener(app.clone(), state.0.clone());
    persist::spawn_persister(app.clone(), state.0.clone());
    apply_autostart(&app, state.0.lock().unwrap().settings.auto_start);
    Ok(())
}

/// 退出：下线广播并退出进程（托盘菜单调用）
#[tauri::command]
fn quit_app(app: tauri::AppHandle, state: tauri::State<AppState>) {
    {
        let mut s = state.0.lock().unwrap();
        s.running = false;
    }
    persist::save_messages(&app, &state.0.clone());
    discovery::broadcast_bye(&state.0.clone());
    app.exit(0);
}

/// 停止网络（保留界面）
#[tauri::command]
fn stop(app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    {
        let mut s = state.0.lock().unwrap();
        s.running = false;
        s.peers.clear();
    }
    persist::save_messages(&app, &state.0.clone());
    discovery::broadcast_bye(&state.0.clone());
    let _ = app.emit("peers", Vec::<Peer>::new());
    Ok(())
}

// ---- 查询 ----

#[tauri::command]
fn list_peers(state: tauri::State<AppState>) -> Vec<Peer> {
    let s = state.0.lock().unwrap();
    s.peers.values().cloned().collect()
}

#[tauri::command]
fn get_self(state: tauri::State<AppState>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    serde_json::json!({
        "uid": s.self_uid,
        "nick": s.settings.nick,
        "group": s.settings.group,
        "host": host_name(),
        "ip": local_ip(),
        "port": s.self_port,
        "status": s.settings.status,
        "inbox": s.inbox_dir,
    })
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Settings {
    state.0.lock().unwrap().settings.clone()
}

#[tauri::command]
fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    settings: Settings,
) -> Result<(), String> {
    let old_auto = state.0.lock().unwrap().settings.auto_start;
    persist_settings(&app, &settings)?;
    {
        let mut s = state.0.lock().unwrap();
        s.settings = settings.clone();
    }
    if settings.auto_start != old_auto {
        apply_autostart(&app, settings.auto_start);
    }
    discovery::refresh(&state.0.clone());
    let _ = app.emit("settings-changed", settings);
    Ok(())
}

#[tauri::command]
fn set_alias(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    uid: String,
    alias: String,
) -> Result<(), String> {
    let mut settings = state.0.lock().unwrap().settings.clone();
    if alias.trim().is_empty() {
        settings.aliases.remove(&uid);
    } else {
        settings.aliases.insert(uid, alias.trim().to_string());
    }
    persist_settings(&app, &settings)?;
    {
        let mut s = state.0.lock().unwrap();
        s.settings = settings.clone();
    }
    let _ = app.emit("settings-changed", settings);
    Ok(())
}

#[tauri::command]
fn list_messages(state: tauri::State<AppState>, uid: String) -> Vec<ChatMessage> {
    state
        .0
        .lock()
        .unwrap()
        .messages
        .get(&uid)
        .cloned()
        .unwrap_or_default()
}

/// 全部文件传输记录（传输管理窗口用）
#[tauri::command]
fn list_transfers(state: tauri::State<AppState>) -> Vec<ChatMessage> {
    let s = state.0.lock().unwrap();
    let mut all: Vec<ChatMessage> = s
        .messages
        .values()
        .flatten()
        .filter(|m| m.kind == "file")
        .cloned()
        .collect();
    all.sort_by(|a, b| b.ts.cmp(&a.ts));
    all.truncate(100);
    all
}

/// 最近会话列表（移动端"消息"页用）
#[tauri::command]
fn list_sessions(state: tauri::State<AppState>) -> serde_json::Value {
    let s = state.0.lock().unwrap();
    let mut out = Vec::new();
    for (uid, list) in s.messages.iter() {
        if let Some(last) = list.last() {
            out.push(serde_json::json!({ "uid": uid, "last": last, "count": list.len() }));
        }
    }
    out.sort_by(|a, b| {
        let ta = a["last"]["ts"].as_u64().unwrap_or(0);
        let tb = b["last"]["ts"].as_u64().unwrap_or(0);
        tb.cmp(&ta)
    });
    serde_json::json!(out)
}

// ---- 消息与传输 ----

#[tauri::command]
fn send_text(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    peer_uid: String,
    text: String,
    pass: Option<String>,
) -> Result<String, String> {
    transfer::send_text(app, state.0.clone(), peer_uid, text, pass)
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

#[tauri::command]
fn begin_file_bytes(state: tauri::State<AppState>, name: String) -> Result<String, String> {
    transfer::begin_file_bytes(state, name)
}

#[tauri::command]
fn push_file_chunk(
    state: tauri::State<AppState>,
    transfer_id: String,
    data: String,
) -> Result<(), String> {
    transfer::push_file_chunk(state, transfer_id, data)
}

#[tauri::command]
fn finish_file_bytes(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
    transfer_id: String,
    peer_uid: String,
) -> Result<String, String> {
    transfer::finish_file_bytes(app, state, transfer_id, peer_uid)
}

/// 对文件接收弹窗做出决定
#[tauri::command]
fn respond_offer(
    state: tauri::State<AppState>,
    transfer_id: String,
    accept: bool,
    save_path: Option<String>,
) -> Result<(), String> {
    let offer = state
        .0
        .lock()
        .unwrap()
        .pending_offers
        .remove(&transfer_id)
        .ok_or_else(|| "该传输已过期或不存在".to_string())?;
    let resp = if accept { save_path } else { None };
    offer.resp.send(resp).map_err(|_| "传输连接已断开".to_string())
}

/// 口令解密（前端展示密文消息用）
#[tauri::command]
fn decrypt_text(cipher_b64: String, pass: String) -> Option<String> {
    network::protocol::decrypt_text(&cipher_b64, &pass)
}

// ---- 窗口与杂项 ----

#[tauri::command]
fn open_path(path: String, reveal: Option<bool>) -> Result<(), String> {
    if reveal.unwrap_or(false) {
        transfer::open_path(&path)
    } else {
        open_with_default_app(&path)
    }
}

/// 用系统默认程序打开文件
fn open_with_default_app(path: &str) -> Result<(), String> {
    let p = Path::new(path);
    if !p.exists() {
        return Err("文件不存在".to_string());
    }
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        let cmd = if cfg!(target_os = "macos") {
            "open"
        } else if cfg!(target_os = "windows") {
            "cmd"
        } else {
            "xdg-open"
        };
        let mut command = std::process::Command::new(cmd);
        if cfg!(target_os = "windows") {
            command.args(["/c", "start", "", path]);
        } else {
            command.arg(path);
        }
        command
            .status()
            .map(|_| ())
            .map_err(|e| format!("打开失败: {}", e))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        let _ = p;
        Ok(())
    }
}

#[tauri::command]
fn win_minimize(window: tauri::Window) -> Result<(), String> {
    #[cfg(desktop)]
    {
        return window.minimize().map_err(|e| e.to_string());
    }
    #[cfg(not(desktop))]
    {
        let _ = window;
        Ok(())
    }
}

#[tauri::command]
fn win_close(window: tauri::Window) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

/// 打开（或聚焦）与某联系人的聊天窗口；移动端改为事件通知前端展示覆盖层
#[tauri::command]
fn open_chat_window(app: tauri::AppHandle, state: tauri::State<AppState>, uid: String) -> Result<(), String> {
    #[cfg(mobile)]
    {
        let _ = state;
        let _ = app.emit("open-chat", uid);
        return Ok(());
    }
    #[cfg(desktop)]
    {
        // 单窗口架构：桌面端由前端直接在主区内嵌聊天视图，无需创建窗口
        let _ = (app, state, uid);
        Ok(())
    }
}

/// 平台标识（前端据此适配 macOS 系统交通灯等）
#[tauri::command]
fn get_platform() -> String {
    if cfg!(target_os = "macos") {
        "macos".into()
    } else if cfg!(target_os = "windows") {
        "windows".into()
    } else if cfg!(target_os = "android") {
        "android".into()
    } else {
        "linux".into()
    }
}

/// 立即广播一次在线通告（刷新按钮）
#[tauri::command]
fn refresh_peers(state: tauri::State<AppState>) {
    discovery::refresh(&state.0.clone());
}

// ---- 应用装配 ----

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ));
    }
    builder
        .manage(state)
        .setup(|app| {
            // 系统托盘：左键显示主窗口，右键菜单（仅桌面端）
            #[cfg(desktop)]
            {
                let handle = app.handle().clone();
                let menu = Menu::with_items(
                    &handle,
                    &[
                        &MenuItem::with_id(&handle, "tray_open", "打开主界面", true, None::<&str>)?,
                        &MenuItem::with_id(&handle, "tray_quit", "退出鸿升飞书", true, None::<&str>)?,
                    ],
                )?;
                let mut tray_builder = TrayIconBuilder::new()
                    .tooltip("鸿升飞书")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| match event.id().as_ref() {
                        "tray_open" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                        "tray_quit" => {
                            if let Some(st) = app.try_state::<AppState>() {
                                {
                                    let mut s = st.0.lock().unwrap();
                                    s.running = false;
                                }
                                discovery::broadcast_bye(&st.0.clone());
                            }
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let tauri::tray::TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            button_state: tauri::tray::MouseButtonState::Up,
                            ..
                        } = event
                        {
                            if let Some(w) = tray.app_handle().get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                    });
                if let Some(icon) = app.default_window_icon() {
                    tray_builder = tray_builder.icon(icon.clone());
                }
                let tray = tray_builder.build(&handle)?;
                let _ = TRAY_ICON.set(tray);
            }
            #[cfg(not(desktop))]
            let _ = app;
            Ok(())
        })
        // 关闭主窗口 = 最小化到托盘（飞鸽传书行为）
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            start, stop, quit_app,
            list_peers, get_self, get_settings, save_settings, set_alias,
            list_messages, list_transfers, list_sessions,
            send_text, send_file, begin_file_bytes, push_file_chunk, finish_file_bytes,
            respond_offer, decrypt_text,
            open_path, win_minimize, win_close,
            open_chat_window, get_platform, refresh_peers,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
