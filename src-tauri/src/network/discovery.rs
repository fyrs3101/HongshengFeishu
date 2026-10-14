// UDP 广播发现：周期性广播自身在线，监听他人广播，过期自动下线
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::Emitter;

use crate::network::protocol::*;
use crate::state::InnerState;

/// 启动发现（广播线程 + 监听线程）
pub fn start_discovery(app: tauri::AppHandle, state: Arc<Mutex<InnerState>>) {
    let st_broadcast = state.clone();
    let st_listen = state.clone();
    let app_listen = app.clone();

    // 广播线程：每 3 秒广播一次；隐身状态不广播（但仍接收他人）
    thread::spawn(move || {
        let socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(_) => return,
        };
        let _ = socket.set_broadcast(true);

        loop {
            let (running, uid, nick, group, port, status) = {
                let s = st_broadcast.lock().unwrap();
                (
                    s.running,
                    s.self_uid.clone(),
                    s.settings.nick.clone(),
                    s.settings.group.clone(),
                    s.self_port,
                    s.settings.status.clone(),
                )
            };
            if !running {
                break;
            }
            if status != "invisible" {
                let announce = Announce {
                    magic: MAGIC.to_string(),
                    ver: PROTOCOL_VERSION,
                    kind: "online".to_string(),
                    uid: uid.clone(),
                    nick,
                    group,
                    host: host_name(),
                    port,
                    os: os_name(),
                    status: status.clone(),
                    app: "HongShengFeishu".to_string(),
                };
                if let Ok(p) = serde_json::to_vec(&announce) {
                    let _ = socket.send_to(&p, DISCOVERY_ADDR);
                }
            }
            thread::sleep(Duration::from_secs(3));
        }
    });

    // 监听线程：接收广播，维护联系人表，清理过期联系人
    thread::spawn(move || {
        let sock = match UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)) {
            Ok(s) => s,
            Err(_) => return,
        };
        let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));

        let mut buf = [0u8; 65536];
        let mut last_prune = now_ms();
        loop {
            if !st_listen.lock().unwrap().running {
                break;
            }
            match sock.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    if let Ok(ann) = serde_json::from_slice::<Announce>(&buf[..n]) {
                        if !ann.valid() {
                            continue;
                        }
                        let self_uid = st_listen.lock().unwrap().self_uid.clone();
                        if ann.uid == self_uid {
                            continue;
                        }
                        let ip = addr.ip().to_string();

                        // 下线广播：立即移除
                        if ann.kind == "bye" {
                            let existed = {
                                let mut s = st_listen.lock().unwrap();
                                s.peers.remove(&ann.uid).is_some()
                            };
                            if existed {
                                emit_peers(&app_listen, &st_listen);
                            }
                            continue;
                        }

                        let peer = Peer {
                            uid: ann.uid.clone(),
                            nick: ann.nick.clone(),
                            group: ann.group.clone(),
                            host: ann.host.clone(),
                            ip,
                            port: ann.port,
                            os: ann.os.clone(),
                            status: ann.status.clone(),
                            last_seen: now_ms(),
                        };
                        let is_new = {
                            let mut s = st_listen.lock().unwrap();
                            let was = s.peers.contains_key(&peer.uid);
                            s.peers.insert(peer.uid.clone(), peer);
                            was
                        };
                        emit_peers(&app_listen, &st_listen);

                        // 对新联系人单播回一个 hello，确保对方立即发现我们
                        if is_new {
                            unicast_hello(&st_listen, &addr.ip().to_string());
                        }
                    }
                }
                Err(_) => {}
            }
            // 定期清理超期未广播的联系人
            let now = now_ms();
            if now - last_prune >= 2_000 {
                last_prune = now;
                let changed = {
                    let mut s = st_listen.lock().unwrap();
                    let before = s.peers.len();
                    s.peers
                        .retain(|_, p| now.saturating_sub(p.last_seen) < PEER_TIMEOUT_MS);
                    s.peers.len() != before
                };
                if changed {
                    emit_peers(&app_listen, &st_listen);
                }
            }
        }
    });
}

fn emit_peers(app: &tauri::AppHandle, state: &Arc<Mutex<InnerState>>) {
    let peers: Vec<Peer> = state.lock().unwrap().peers.values().cloned().collect();
    #[cfg(desktop)]
    crate::update_tray_tooltip(app, peers.len());
    let _ = app.emit("peers", peers);
}

/// 向指定 IP 单播一个 hello（让对方立即知道我们在线及端口）
pub fn unicast_hello(state: &Arc<Mutex<InnerState>>, ip: &str) {
    let (uid, nick, group, port, status) = {
        let s = state.lock().unwrap();
        (
            s.self_uid.clone(),
            s.settings.nick.clone(),
            s.settings.group.clone(),
            s.self_port,
            s.settings.status.clone(),
        )
    };
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        let _ = socket.set_broadcast(true);
        let hello = Announce {
            magic: MAGIC.to_string(),
            ver: PROTOCOL_VERSION,
            kind: "hello".to_string(),
            uid,
            nick,
            group,
            host: host_name(),
            port,
            os: os_name(),
            status,
            app: "HongShengFeishu".to_string(),
        };
        if let Ok(p) = serde_json::to_vec(&hello) {
            let _ = socket.send_to(&p, (ip, DISCOVERY_PORT));
        }
    }
}

/// 下线广播（退出时调用）
pub fn broadcast_bye(state: &Arc<Mutex<InnerState>>) {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        let _ = socket.set_broadcast(true);
        let (uid, nick, group, port) = {
            let s = state.lock().unwrap();
            (
                s.self_uid.clone(),
                s.settings.nick.clone(),
                s.settings.group.clone(),
                s.self_port,
            )
        };
        let bye = Announce {
            magic: MAGIC.to_string(),
            ver: PROTOCOL_VERSION,
            kind: "bye".to_string(),
            uid,
            nick,
            group,
            host: host_name(),
            port,
            os: os_name(),
            status: "offline".to_string(),
            app: "HongShengFeishu".to_string(),
        };
        if let Ok(p) = serde_json::to_vec(&bye) {
            let _ = socket.send_to(&p, DISCOVERY_ADDR);
        }
    }
}

/// 立即广播一次在线通告（刷新按钮用）
pub fn refresh(state: &Arc<Mutex<InnerState>>) {
    let (uid, nick, group, port, status) = {
        let s = state.lock().unwrap();
        (
            s.self_uid.clone(),
            s.settings.nick.clone(),
            s.settings.group.clone(),
            s.self_port,
            s.settings.status.clone(),
        )
    };
    if status == "invisible" {
        return;
    }
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        let _ = socket.set_broadcast(true);
        let announce = Announce {
            magic: MAGIC.to_string(),
            ver: PROTOCOL_VERSION,
            kind: "online".to_string(),
            uid,
            nick,
            group,
            host: host_name(),
            port,
            os: os_name(),
            status,
            app: "HongShengFeishu".to_string(),
        };
        if let Ok(p) = serde_json::to_vec(&announce) {
            let _ = socket.send_to(&p, DISCOVERY_ADDR);
        }
    }
}
