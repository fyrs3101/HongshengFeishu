// UDP 广播发现：周期性广播自身在线，并监听他人广播
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

    // 广播线程
    thread::spawn(move || {
        let socket = match UdpSocket::bind("0.0.0.0:0") {
            Ok(s) => s,
            Err(_) => return,
        };
        let _ = socket.set_broadcast(true);

        let uid = st_broadcast.lock().unwrap().self_uid.clone();
        let nick = st_broadcast.lock().unwrap().self_nick.clone();
        let port = st_broadcast.lock().unwrap().self_port;
        let host = host_name();
        let os = os_name();

        let announce = Announce {
            magic: MAGIC.to_string(),
            ver: PROTOCOL_VERSION,
            kind: "online".to_string(),
            uid: uid.clone(),
            nick,
            host,
            port,
            os,
            app: "HongShengFeishu".to_string(),
        };
        let payload = serde_json::to_vec(&announce).unwrap_or_default();

        while st_broadcast.lock().unwrap().running {
            let _ = socket.send_to(&payload, DISCOVERY_ADDR);
            thread::sleep(Duration::from_secs(3));
        }
    });

    // 监听线程
    thread::spawn(move || {
        let sock = match UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)) {
            Ok(s) => s,
            Err(_) => return,
        };
        let _ = sock.set_read_timeout(Some(Duration::from_millis(600)));

        let mut buf = [0u8; 65536];
        while st_listen.lock().unwrap().running {
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
                        let peer = Peer {
                            uid: ann.uid.clone(),
                            nick: ann.nick.clone(),
                            host: ann.host.clone(),
                            ip,
                            port: ann.port,
                            os: ann.os.clone(),
                            last_seen: now_ms(),
                        };
                        let is_new = {
                            let mut s = st_listen.lock().unwrap();
                            let was = s.peers.contains_key(&peer.uid);
                            s.peers.insert(peer.uid.clone(), peer);
                            was
                        };
                        // 下发联系人列表
                        let peers: Vec<Peer> =
                            st_listen.lock().unwrap().peers.values().cloned().collect();
                        let _ = app_listen.emit("peers", peers);

                        // 对新联系人单独回一个 hello，确保对方也能发现我们
                        if is_new {
                            if let Ok(reply) = UdpSocket::bind("0.0.0.0:0") {
                                let _ = reply.set_broadcast(true);
                                let hello = Announce {
                                    magic: MAGIC.to_string(),
                                    ver: PROTOCOL_VERSION,
                                    kind: "hello".to_string(),
                                    uid: self_uid.clone(),
                                    nick: st_listen.lock().unwrap().self_nick.clone(),
                                    host: host_name(),
                                    port: st_listen.lock().unwrap().self_port,
                                    os: os_name(),
                                    app: "HongShengFeishu".to_string(),
                                };
                                if let Ok(p) = serde_json::to_vec(&hello) {
                                    let _ = reply.send_to(&p, (addr.ip(), DISCOVERY_PORT));
                                }
                            }
                        }
                    }
                }
                Err(_) => continue,
            }
        }
    });
}

/// 下线广播（应用退出时调用）
pub fn broadcast_bye(state: &Arc<Mutex<InnerState>>) {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        let _ = socket.set_broadcast(true);
        let (uid, nick, port) = {
            let s = state.lock().unwrap();
            (s.self_uid.clone(), s.self_nick.clone(), s.self_port)
        };
        let bye = Announce {
            magic: MAGIC.to_string(),
            ver: PROTOCOL_VERSION,
            kind: "bye".to_string(),
            uid,
            nick,
            host: host_name(),
            port,
            os: os_name(),
            app: "HongShengFeishu".to_string(),
        };
        if let Ok(p) = serde_json::to_vec(&bye) {
            let _ = socket.send_to(&p, DISCOVERY_ADDR);
        }
    }
}
