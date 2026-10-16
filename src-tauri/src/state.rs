// 全局状态：联系人表 + 会话消息 + 设置 + 待处理文件接收 + 聊天窗口注册表
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::network::protocol::{ChatMessage, Peer};

/// 持久化设置（settings.json）
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub nick: String,
    pub group: String,
    pub status: String, // online | away | invisible
    pub sound: bool,
    pub auto_accept: bool,
    pub auto_start: bool,
    pub inbox_dir: Option<String>,
    #[serde(default)]
    pub aliases: HashMap<String, String>, // peer_uid -> 备注名
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            nick: "鸿升周老师".to_string(),
            group: "鸿升组".to_string(),
            status: "online".to_string(),
            sound: true,
            auto_accept: false,
            auto_start: false,
            inbox_dir: None,
            aliases: HashMap::new(),
        }
    }
}

/// 文件接收确认请求（等待前端决定）
#[allow(dead_code)]
pub struct PendingOffer {
    pub from: String,
    pub nick: String,
    pub transfer_id: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    /// 前端决定：Some(保存路径) = 接收；None = 拒绝
    pub resp: Sender<Option<String>>,
}

#[derive(Default)]
pub struct InnerState {
    pub self_uid: String,
    pub self_port: u16,
    pub peers: HashMap<String, Peer>,
    pub messages: HashMap<String, Vec<ChatMessage>>,
    /// 消息表有变动，持久化线程据此节流落盘
    pub messages_dirty: bool,
    pub running: bool,
    /// 收件箱根目录（运行期确定）
    pub inbox_dir: String,
    pub settings: Settings,
    /// 等待接收确认的传输 transfer_id -> offer
    pub pending_offers: HashMap<String, PendingOffer>,
    /// 被拒绝的传输（接收方拒绝或发送方被拒），分片到达时直接丢弃
    pub rejected: HashSet<String>,
    /// 已打开的聊天窗口 peer_uid -> window label（保留兼容，单窗口架构下不再使用）
    #[allow(dead_code)]
    pub chat_windows: HashMap<String, String>,
    /// 移动端分块上传暂存：transfer_id -> 临时文件路径
    pub staging: HashMap<String, PathBuf>,
    // ---- 网络诊断计数器（net_diag 命令输出，供用户自查"为什么发现不了"） ----
    /// 已发送的发现广播帧数
    pub udp_sent: u64,
    /// 已接收的发现广播帧数
    pub udp_recv: u64,
    /// UDP 发现监听端口是否绑定成功
    pub listener_ok: bool,
    /// 防火墙提示是否已在本运行周期内提示过（避免每次启动都弹）
    pub fw_hint_shown: bool,
}

/// 防中毒加锁：任一线程在持锁时 panic 会毒化 Mutex，
/// 导致发现/监听线程后续全部 lock().unwrap() 级联 panic 而静默死亡
/// （表现为"自动与刷新都发现不了"）。这里统一改为恢复锁内容。
pub fn lock_state(s: &Mutex<InnerState>) -> std::sync::MutexGuard<'_, InnerState> {
    s.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Clone)]
pub struct AppState(pub Arc<Mutex<InnerState>>);

impl AppState {
    pub fn new() -> Self {
        AppState(Arc::new(Mutex::new(InnerState::default())))
    }
}
