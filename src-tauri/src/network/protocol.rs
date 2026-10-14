// 鸿升飞书协议层（HSFP v2：HongSheng File-transfer Protocol）
// v2 新增：组名 / 在线状态 / 消息回执 / 口令加密 / 传输速度与剩余时间 / 接收确认
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const MAGIC: &str = "HSFP";
pub const PROTOCOL_VERSION: u8 = 2;
/// UDP 广播发现端口
pub const DISCOVERY_PORT: u16 = 24680;
/// 子网广播地址
pub const DISCOVERY_ADDR: &str = "255.255.255.255:24680";
/// 联系人离线判定：超过 13 秒（约 4 个广播周期）未见到广播视为下线
pub const PEER_TIMEOUT_MS: u64 = 13_000;

static UID_SEQ: AtomicU64 = AtomicU64::new(1);

/// 当前时间戳（毫秒）
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 生成唯一标识（进程内单调递增，避免同毫秒碰撞）
pub fn gen_uid() -> String {
    format!(
        "{:x}-{:x}-{:x}",
        std::process::id(),
        now_ms() % 1_000_000_000,
        UID_SEQ.fetch_add(1, Ordering::Relaxed)
    )
}

/// 人类可读大小
#[allow(dead_code)]
pub fn readable_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut i = 0;
    while size >= 1024.0 && i < UNITS.len() - 1 {
        size /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", size, UNITS[i])
    }
}

/// UDP 发现报文
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Announce {
    pub magic: String,
    pub ver: u8,
    pub kind: String, // online | hello | bye
    pub uid: String,
    pub nick: String,
    pub group: String,
    pub host: String,
    pub port: u16,
    pub os: String,
    pub status: String, // online | away | invisible
    pub app: String,
}

impl Announce {
    pub fn valid(&self) -> bool {
        self.magic == MAGIC && self.ver == PROTOCOL_VERSION
    }
}

/// 在线联系人
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Peer {
    pub uid: String,
    pub nick: String,
    pub group: String,
    pub host: String,
    pub ip: String,
    pub port: u16,
    pub os: String,
    pub status: String,
    pub last_seen: u64,
}

/// 聊天消息 / 传输记录
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChatMessage {
    pub id: String,
    pub peer_uid: String,
    pub direction: String, // out | in
    pub kind: String,      // text | file
    pub text: String,
    #[serde(default)]
    pub enc: bool, // 是否为密文（需口令解密）
    pub file_name: String,
    pub file_size: u64,
    pub is_dir: bool,
    pub transfer_id: String,
    pub status: String, // sending | sent | delivered | receiving | received | failed | rejected
    pub progress: u64,  // 0..100
    #[serde(default)]
    pub speed_bps: u64, // 传输速度（字节/秒）
    #[serde(default)]
    pub eta_secs: u64, // 预计剩余秒数
    #[serde(default)]
    pub saved_path: String, // 接收方保存路径（收到文件后可用"打开"）
    pub ts: u64,
}

impl ChatMessage {
    pub fn text_out(peer_uid: &str, text: &str, enc: bool) -> Self {
        ChatMessage {
            id: gen_uid(),
            peer_uid: peer_uid.to_string(),
            direction: "out".into(),
            kind: "text".into(),
            text: text.to_string(),
            enc,
            file_name: String::new(),
            file_size: 0,
            is_dir: false,
            transfer_id: String::new(),
            status: "sending".into(),
            progress: 100,
            speed_bps: 0,
            eta_secs: 0,
            saved_path: String::new(),
            ts: now_ms(),
        }
    }

    pub fn text_in(peer_uid: &str, text: &str, enc: bool) -> Self {
        ChatMessage {
            id: gen_uid(),
            peer_uid: peer_uid.to_string(),
            direction: "in".into(),
            kind: "text".into(),
            text: text.to_string(),
            enc,
            file_name: String::new(),
            file_size: 0,
            is_dir: false,
            transfer_id: String::new(),
            status: "received".into(),
            progress: 100,
            speed_bps: 0,
            eta_secs: 0,
            saved_path: String::new(),
            ts: now_ms(),
        }
    }
}

/// TCP 分帧报文（JSON + 4 字节大端长度前缀）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Frame {
    Text {
        from: String,
        nick: String,
        msg_id: String,
        text: String,
        #[serde(default)]
        cipher: Option<String>, // 密文（base64），与 text 二选一
    },
    FileMeta {
        from: String,
        nick: String,
        transfer_id: String,
        name: String,
        size: u64,
        is_dir: bool,
    },
    FileChunk {
        transfer_id: String,
        index: u64,
        total: u64,
        data: String, // base64
    },
    FileDone {
        transfer_id: String,
    },
    Ack {
        msg_id: Option<String>,
        transfer_id: String,
    },
    Reject {
        transfer_id: String,
    },
}

/// 编码一帧：4 字节大端长度 + JSON
pub fn encode_frame(frame: &Frame) -> Vec<u8> {
    let json = serde_json::to_vec(frame).unwrap_or_default();
    let len = json.len() as u32;
    let mut out = len.to_be_bytes().to_vec();
    out.extend_from_slice(&json);
    out
}

/// 从流中读取一帧（阻塞直到完整一帧或 EOF）
pub fn read_frame(stream: &mut impl Read) -> Option<Frame> {
    let mut len_buf = [0u8; 4];
    if stream.read_exact(&mut len_buf).is_err() {
        return None;
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    if len == 0 || len > 64 * 1024 * 1024 {
        return None;
    }
    let mut buf = vec![0u8; len];
    if stream.read_exact(&mut buf).is_err() {
        return None;
    }
    serde_json::from_slice(&buf).ok()
}

/// 把一帧写入流
pub fn write_frame(stream: &mut impl Write, frame: &Frame) -> std::io::Result<()> {
    let bytes = encode_frame(frame);
    stream.write_all(&bytes)
}

// ---- 口令加密（SHA-256 密钥流异或，局域网自用级防护）----

fn keystream(pass: &str, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len);
    let mut counter: u64 = 0;
    while out.len() < len {
        let mut h = Sha256::new();
        h.update(pass.as_bytes());
        h.update(counter.to_be_bytes());
        out.extend_from_slice(&h.finalize());
        counter += 1;
    }
    out.truncate(len);
    out
}

/// 加密：口令 → 密文（base64）。同一口令可逆。
pub fn encrypt_text(text: &str, pass: &str) -> String {
    let bytes = text.as_bytes();
    let ks = keystream(pass, bytes.len());
    let cipher: Vec<u8> = bytes.iter().zip(ks).map(|(b, k)| b ^ k).collect();
    B64.encode(cipher)
}

/// 解密：密文（base64）+ 口令 → 明文。口令错误返回 None（上层按乱码/失败处理）。
pub fn decrypt_text(cipher_b64: &str, pass: &str) -> Option<String> {
    let bytes = B64.decode(cipher_b64).ok()?;
    let ks = keystream(pass, bytes.len());
    let plain: Vec<u8> = bytes.iter().zip(ks).map(|(b, k)| b ^ k).collect();
    String::from_utf8(plain).ok()
}

/// 操作系统名
pub fn os_name() -> String {
    std::env::consts::OS.to_string()
}

/// 本机主机名（尽力获取，跨平台）
pub fn host_name() -> String {
    for var in ["HOSTNAME", "COMPUTERNAME"] {
        if let Ok(v) = std::env::var(var) {
            if !v.trim().is_empty() {
                return v.trim().to_string();
            }
        }
    }
    if let Ok(s) = std::fs::read_to_string("/etc/hostname") {
        let s = s.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    if let Ok(out) = std::process::Command::new("hostname").output() {
        if out.status.success() {
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    "unknown".to_string()
}

/// 本机局域网 IP（UDP connect 技巧，不产生实际流量）
pub fn local_ip() -> String {
    if let Ok(sock) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                let ip = addr.ip().to_string();
                if ip != "0.0.0.0" {
                    return ip;
                }
            }
        }
    }
    "127.0.0.1".to_string()
}
