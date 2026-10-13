// 全局状态：联系人表 + 会话消息 + 运行标志
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::network::protocol::{ChatMessage, Peer};

#[derive(Default)]
pub struct InnerState {
    pub self_uid: String,
    pub self_nick: String,
    pub self_port: u16,
    pub peers: HashMap<String, Peer>,
    pub messages: HashMap<String, Vec<ChatMessage>>,
    pub running: bool,
    /// 收件箱根目录（运行期设置）
    pub inbox_dir: String,
}

#[derive(Clone)]
pub struct AppState(pub Arc<Mutex<InnerState>>);

impl AppState {
    pub fn new() -> Self {
        AppState(Arc::new(Mutex::new(InnerState::default())))
    }
}
