// UDP 广播发现：周期性广播自身在线，监听他人广播，过期自动下线
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use tauri::Emitter;

use crate::network::protocol::*;
use crate::state::{lock_state, InnerState};

/// 枚举本机所有"发得出广播"的网卡（UP + 非回环 + 支持广播），
/// 计算各自的子网定向广播地址（IP | ~掩码）。
///
/// 为什么需要：Android 上向 255.255.255.255（有限广播）发送经常失败
/// （系统限制部分接口的有限广播），而华为等机型的省电策略还会过滤广播报文；
/// 桌面多网卡场景下有限广播也只走默认路由。定向广播（如 192.168.31.255:24680）
/// 是局域网发现的标准可靠做法（飞鸽传书帮助文档同样要求同网段互通）。
#[cfg(unix)]
fn subnet_broadcast_addrs() -> Vec<SocketAddr> {
    let mut out: Vec<SocketAddr> = Vec::new();
    unsafe {
        let mut ifap: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut ifap) != 0 || ifap.is_null() {
            return out;
        }
        let mut cur = ifap;
        while !cur.is_null() {
            let e = &*cur;
            let flags = e.ifa_flags as i32;
            let up = (flags & libc::IFF_UP) != 0;
            let no_loopback = (flags & libc::IFF_LOOPBACK) == 0;
            let can_bcast = (flags & libc::IFF_BROADCAST) != 0;
            let has_addr = !e.ifa_addr.is_null()
                && (*e.ifa_addr).sa_family == libc::AF_INET as libc::sa_family_t;
            let has_mask = !e.ifa_netmask.is_null()
                && (*e.ifa_netmask).sa_family == libc::AF_INET as libc::sa_family_t;
            if up && no_loopback && can_bcast && has_addr && has_mask {
                let ip = (*(e.ifa_addr as *const libc::sockaddr_in)).sin_addr.s_addr;
                let mask = (*(e.ifa_netmask as *const libc::sockaddr_in)).sin_addr.s_addr;
                let bcast = ip | !mask;
                let addr = SocketAddr::new(
                    std::net::IpAddr::V4(std::net::Ipv4Addr::from(u32::from_be(bcast))),
                    DISCOVERY_PORT,
                );
                if !out.contains(&addr) {
                    out.push(addr);
                }
            }
            cur = e.ifa_next;
        }
        libc::freeifaddrs(ifap);
    }
    out
}

#[cfg(not(unix))]
fn subnet_broadcast_addrs() -> Vec<SocketAddr> {
    Vec::new()
}

/// 广播目标集合：各网段定向广播 + 有限广播 255.255.255.255（桌面端有效，
/// 安卓上失败也无妨——定向广播已覆盖）。发送时逐个尝试，全部忽略错误。
fn announce_targets() -> Vec<SocketAddr> {
    let mut v = subnet_broadcast_addrs();
    if let Ok(a) = DISCOVERY_ADDR.parse::<SocketAddr>() {
        if !v.contains(&a) {
            v.push(a);
        }
    }
    v
}

/// 广播目标集合（供诊断命令展示：用户可据此核对与对端是否同网段）
pub fn broadcast_targets_debug() -> Vec<String> {
    announce_targets()
        .iter()
        .map(|a| a.to_string())
        .collect()
}

/// 向所有广播目标发送一帧通告
fn send_to_all(socket: &UdpSocket, payload: &[u8]) {
    for target in announce_targets() {
        let _ = socket.send_to(payload, target);
    }
}

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

        // 启动突发：t=0 / +0.3s / +0.9s 连发三帧，
        // 让对端几乎在启动瞬间就能看到我们（旧实现要等最多 3 秒的首帧）
        for delay in [0u64, 300, 900] {
            if delay > 0 {
                thread::sleep(Duration::from_millis(delay));
            }
            let (running, uid, nick, group, port, status) = {
                let s = lock_state(&st_broadcast);
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
                return;
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
                    send_to_all(&socket, &p);
                    lock_state(&st_broadcast).udp_sent += 1;
                }
            }
        }

        loop {
            let (running, uid, nick, group, port, status) = {
                let s = lock_state(&st_broadcast);
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
                    send_to_all(&socket, &p);
                    lock_state(&st_broadcast).udp_sent += 1;
                }
            }
            thread::sleep(Duration::from_secs(3));
        }
    });

    // 监听线程：接收广播，维护联系人表，清理过期联系人
    thread::spawn(move || {
        // 绑定失败（端口被其他软件/残留实例占用）不退出：每 2 秒重试，
        // 直到成功或 running=false——旧代码直接 return，会导致本次运行
        // 永远收不到任何广播（自动与刷新都发现不了的直接原因之一）
        let sock = loop {
            match UdpSocket::bind(format!("0.0.0.0:{}", DISCOVERY_PORT)) {
                Ok(s) => break s,
                Err(_) => {
                    lock_state(&st_listen).listener_ok = false;
                    if !lock_state(&st_listen).running {
                        return;
                    }
                    thread::sleep(Duration::from_secs(2));
                }
            }
        };
        lock_state(&st_listen).listener_ok = true;
        let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));

        let mut buf = [0u8; 65536];
        let mut last_prune = now_ms();
        loop {
            if !lock_state(&st_listen).running {
                break;
            }
            match sock.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    if let Ok(ann) = serde_json::from_slice::<Announce>(&buf[..n]) {
                        if !ann.valid() {
                            continue;
                        }
                        lock_state(&st_listen).udp_recv += 1;
                        let self_uid = lock_state(&st_listen).self_uid.clone();
                        if ann.uid == self_uid {
                            continue;
                        }
                        let ip = addr.ip().to_string();

                        // 下线广播：立即移除
                        if ann.kind == "bye" {
                            let existed = {
                                let mut s = lock_state(&st_listen);
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
                            let mut s = lock_state(&st_listen);
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
                    let mut s = lock_state(&st_listen);
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
    let peers: Vec<Peer> = lock_state(&state).peers.values().cloned().collect();
    #[cfg(desktop)]
    crate::update_tray_tooltip(app, peers.len());
    let _ = app.emit("peers", peers);
}

/// 向指定 IP 单播一个 hello（让对方立即知道我们在线及端口）
pub fn unicast_hello(state: &Arc<Mutex<InnerState>>, ip: &str) {
    let (uid, nick, group, port, status) = {
        let s = lock_state(&state);
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
            let s = lock_state(&state);
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
            send_to_all(&socket, &p);
        }
    }
}

/// 立即广播一次在线通告（刷新按钮用）
pub fn refresh(state: &Arc<Mutex<InnerState>>) {
    let (uid, nick, group, port, status) = {
        let s = lock_state(&state);
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
            send_to_all(&socket, &p);
            lock_state(state).udp_sent += 1;
        }
    }
}
