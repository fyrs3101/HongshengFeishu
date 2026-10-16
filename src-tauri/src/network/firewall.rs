// Windows 防火墙：局域网发现的头号环境杀手。
//
// 背景：Windows Defender 防火墙默认拦截所有"未经请求的入站"流量，
// 且 UDP 入站不会像 TCP 监听那样弹窗询问用户——结果是静默拦截：
// 别的机器根本看不到本机，本机也收不到别人的广播，
// 表现为"三端彼此不能发现，无论自动还是刷新都不能发现"。
//
// 对策（按优先级）：
// 1. 启动时静默尝试用 netsh 为本程序添加入站放行规则
//    （用户以管理员身份运行/UAC 已关时直接成功）；
// 2. 静默失败（普通用户无权限）时，通过 notice 事件通知前端，
//    由用户点击"一键修复"：写入批处理 + PowerShell 提权执行（弹 UAC）；
// 3. 仍失败则引导用户手动放行（打开 Windows 安全中心防火墙页）。
//
// 规则按"程序 + 端口"限定，只放行鸿升飞书自己，不影响系统安全面。
#![cfg(target_os = "windows")]

use std::os::windows::process::CommandExt;
use std::path::PathBuf;

use tauri::Emitter;

use crate::network::protocol::{DISCOVERY_PORT, TCP_PORT};
use crate::state::lock_state;
use crate::state::InnerState;
use std::sync::Arc;
use std::sync::Mutex;

/// 不弹出黑色控制台窗口
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 规则名（同时用于 net_diag 检测规则是否已存在）
const RULE_UDP: &str = "鸿升飞书 局域网发现 UDP 24680";
const RULE_TCP: &str = "鸿升飞书 文件传输 TCP 24681";

fn netsh(args: &[&str]) -> std::io::Result<std::process::Output> {
    std::process::Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

/// 检测放行规则是否已存在（netsh show rule 能找到即视为已放行）
pub fn rules_present() -> bool {
    for name in [RULE_UDP, RULE_TCP] {
        let out = match netsh(&["advfirewall", "firewall", "show", "rule", &format!("name={}", name), "dir=in"]) {
            Ok(o) => o,
            Err(_) => return false,
        };
        let text = String::from_utf8_lossy(&out.stdout);
        if !text.contains(name) {
            return false;
        }
    }
    true
}

/// 静默添加放行规则（无 UAC 权限时失败，返回错误信息）
pub fn add_rules_silently() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = exe.display().to_string();
    let mut last_err = String::new();
    for (name, proto, port) in [
        (RULE_UDP, "UDP", DISCOVERY_PORT),
        (RULE_TCP, "TCP", TCP_PORT),
    ] {
        // 先删后加：规则已存在时 add 会报错，删除忽略结果即可
        let _ = netsh(&[
            "advfirewall", "firewall", "delete", "rule",
            &format!("name={}", name),
        ]);
        let out = netsh(&[
            "advfirewall", "firewall", "add", "rule",
            &format!("name={}", name),
            "dir=in",
            "action=allow",
            &format!("protocol={}", proto),
            &format!("localport={}", port),
            &format!("program={}", exe),
            "profile=any",
            "enable=yes",
        ]);
        match out {
            Ok(o) if o.status.success() => {}
            Ok(o) => {
                last_err = String::from_utf8_lossy(&o.stderr).trim().to_string();
            }
            Err(e) => last_err = e.to_string(),
        }
    }
    if last_err.is_empty() {
        Ok(())
    } else {
        Err(last_err)
    }
}

/// 一键修复：写入批处理到临时目录并提权执行（弹 UAC，用户点"是"后生效）。
/// 批处理内置全部 netsh 命令与当前程序路径，执行完暂停显示结果。
pub fn fix_firewall_elevated() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let bat: PathBuf = std::env::temp_dir().join("hongsheng-feishu-firewall.bat");
    let script = format!(
        "@echo off\r\n\
         chcp 65001 >nul\r\n\
         echo 正在为 鸿升飞书 添加 Windows 防火墙放行规则...\r\n\
         netsh advfirewall firewall delete rule name=\"{}\" >nul 2>&1\r\n\
         netsh advfirewall firewall add rule name=\"{}\" dir=in action=allow protocol=UDP localport={} program=\"{}\" profile=any\r\n\
         netsh advfirewall firewall delete rule name=\"{}\" >nul 2>&1\r\n\
         netsh advfirewall firewall add rule name=\"{}\" dir=in action=allow protocol=TCP localport={} program=\"{}\" profile=any\r\n\
         echo.\r\n\
         echo 完成。如上方出现失败信息，请本窗口保持打开并截图反馈。\r\n\
         pause\r\n",
        RULE_UDP,
        RULE_UDP, DISCOVERY_PORT, exe.display(),
        RULE_TCP,
        RULE_TCP, TCP_PORT, exe.display(),
    );
    std::fs::write(&bat, script).map_err(|e| format!("写入修复脚本失败: {}", e))?;

    // 经 PowerShell 以管理员身份启动批处理（触发 UAC 提权）
    let ps = format!(
        "Start-Process -FilePath '{}' -Verb RunAs -WindowStyle Normal",
        bat.display()
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &ps])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("提权启动失败: {}", e))?;
    if out.status.success() {
        Ok(bat.to_string_lossy().to_string())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// 打开 Windows 安全中心防火墙页面（手动放行入口）
pub fn open_firewall_settings() {
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "ms-settings:windowsdefender"])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn();
}

/// 启动时确保放行：先静默尝试；失败则发一次性 notice 事件请前端引导用户。
pub fn ensure_allowed(app: &tauri::AppHandle, state: &Arc<Mutex<InnerState>>) {
    if rules_present() {
        return;
    }
    if add_rules_silently().is_ok() {
        return;
    }
    // 静默失败（无管理员权限）：仅提示一次，避免每次启动都打扰
    let already = {
        let mut s = lock_state(state);
        let v = s.fw_hint_shown;
        s.fw_hint_shown = true;
        v
    };
    if !already {
        let _ = app.emit(
            "notice",
            serde_json::json!({
                "kind": "firewall",
                "title": "Windows 防火墙可能正在拦截局域网发现",
                "body": "Windows 默认拦截入站 UDP 且不会弹窗询问。请点击「一键修复」授权放行，或按指引手动放行 鸿升飞书。",
            }),
        );
    }
}
