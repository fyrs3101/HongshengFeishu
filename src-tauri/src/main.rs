// 鸿升飞书 · Tauri 入口
// 预编译发布版本下隐藏控制台窗口（Windows）。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    hongsheng_feishu_lib::run()
}
