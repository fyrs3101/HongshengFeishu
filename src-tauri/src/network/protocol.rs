// 鸿升飞书协议层（HSFP：HongSheng File-transfer Protocol）
// 设计原则：与早期 Swift 版完全等价，但用 Rust 重写，零第三方网络库。

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAGIC: &str = "HSFP";
pub const PROTOCOL_VERSION: u8 = 1;
/// UDP 广播发现端口
pub const DISCOVERY_PORT: u16 = 24680;
/// 子网广播地址
pub const DISCOVERY_ADDR: &str = "255.255.255.255:24680";
/// TCP 消息/传输默认端口（运行时由系统分配并广播）
pub const TRANSFER_PORT: u16 = 24681;

/// 当前时间戳（毫秒）
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 生成进程级唯一标识
pub fn gen_uid() -> String {
    format!("{:x}-{:x}", std::process::id(), now_ms() % 1_000_000)
}

/// 人类可读大小
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
    pub host: String,
    pub port: u16,
    pub os: String,
    pub app: String,
}

impl Announce {
    pub fn valid(&self) -> bool {
        self.magic == MAGIC
    }
}

/// 在线联系人
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Peer {
    pub uid: String,
    pub nick: String,
    pub host: String,
    pub ip: String,
    pub port: u16,
    pub os: String,
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
    pub file_name: String,
    pub file_size: u64,
    pub is_dir: bool,
    pub transfer_id: String,
    pub status: String, // sending | sent | receiving | received | failed
    pub progress: u64, // 0..100
    pub ts: u64,
}

impl ChatMessage {
    pub fn text_out(peer_uid: &str, text: &str) -> Self {
        ChatMessage {
            id: gen_uid(),
            peer_uid: peer_uid.to_string(),
            direction: "out".into(),
            kind: "text".into(),
            text: text.to_string(),
            file_name: String::new(),
            file_size: 0,
            is_dir: false,
            transfer_id: String::new(),
            status: "sent".into(),
            progress: 100,
            ts: now_ms(),
        }
    }

    pub fn text_in(peer_uid: &str, _nick: &str, text: &str) -> Self {
        ChatMessage {
            id: gen_uid(),
            peer_uid: peer_uid.to_string(),
            direction: "in".into(),
            kind: "text".into(),
            text: text.to_string(),
            file_name: String::new(),
            file_size: 0,
            is_dir: false,
            transfer_id: String::new(),
            status: "received".into(),
            progress: 100,
            ts: now_ms(),
        }
    }
}

/// TCP 分帧报文（JSON + 长度前缀）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Frame {
    Text { from: String, nick: String, text: String },
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

/// 操作系统名
pub fn os_name() -> String {
    std::env::consts::OS.to_string()
}

/// 主机名（尽力获取）
pub fn host_name() -> String {
    std::fs::read_to_string("/etc/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}
