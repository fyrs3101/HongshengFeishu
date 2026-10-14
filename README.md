# 鸿升飞书（Tauri 版）

局域网即时通讯与文件传输工具：自动发现网段内同事、点对点收发消息与文件，**无需服务器**。
界面与功能对标飞鸽传书，但代码、图标、协议均为**原创重写**，版权归属**鸿升周老师**。

> 这是用 **Tauri v2**（Rust 后端 + Web 前端）重写的版本，一套代码产出 macOS / Windows / Android 三端。

## 功能（v1.2.0 飞鸽级）

**发现与状态**

- 局域网 UDP 广播自动发现，13 秒无广播判定离线
- 组名（同组互现）、在线 / 离开 / 隐身三态（隐身不响应发现但可收消息）
- 本机 IP / 主机名 / 操作系统标识展示，备注名（别名）管理

**消息**

- 点对点文本聊天（TCP 直连，JSON 分帧 + 4 字节大端长度前缀）
- 消息回执：发送中 → 已发送 → 已送达
- 口令加密通信（SHA-256 密钥流异或，密文仅本地可解密）
- 群发（一键广播给所有在线联系人）
- 未读计数、消息提示音、系统通知（无窗口聚焦时）
- 聊天记录本地持久化（messages.json，原子写入，重启不丢失，每会话最多保留 1000 条）

**文件**

- 文件 / 文件夹传输（文件夹自动打包 zip，分片 + 进度 + 速度 + 剩余时间）
- 拖拽即传（拖到聊天窗或联系人上）、接收确认弹窗（可拒绝，120 秒超时按拒绝）
- 自动接收模式、收件箱目录自定义、收到后一键打开 / 定位文件夹
- 传输管理窗口实时查看全部传输任务

**体验**

- 系统托盘（关闭主窗口 = 最小化到托盘，左键还原）
- 自定义无边框标题栏（macOS 交通灯）、**单窗口架构**：聊天嵌入主区、设置与传输管理为模态框（彻底规避多窗口白屏/卡死）
- 移动端底部 Tab 布局（联系人 / 消息 / 传输 / 我的）+ 全屏聊天覆盖层
- 设置持久化（settings.json）、开机自启动（桌面端）

## 技术栈

- 后端：Rust（零第三方网络库，自研 HSFP v2 协议）
- 前端：原生 HTML / CSS / JS（Tauri 全局 API 通信）
- 打包：Tauri v2 + GitHub Actions 三平台云构建

## 三种方式拿到安装包

### 1. 自己的 Mac（推荐，一条命令）

```bash
cd HongshengFeishu-Tauri/src-tauri
cargo install tauri-cli --version "^2"   # 首次需装 CLI
cargo tauri build
# 产物：target/release/bundle/dmg/*.dmg 与 target/release/bundle/macos/*.app
```

### 2. 没有 Mac 也能出真包（GitHub Actions 云构建，推荐）

`release.yml` 用官方 `tauri-action` 在 GitHub 运行器上构建 **macOS / Windows / Android** 三平台，
打标签即自动触发：

```bash
git tag v1.2.0 && git push origin v1.2.0
# 到 GitHub → Releases → 下载：
#   HongshengFeishu-1.2.0-macOS-arm64.dmg / .app.tar.gz
#   HongshengFeishu-1.2.0-windows-x64-setup.exe
#   HongshengFeishu-1.2.0-android.apk（华为等手机开启"旁加载"即可安装）
```

### 3. 沙箱里我能验证什么

Linux 沙箱中 `cargo check` 双目标（桌面 + Android）零警告、可本地产出调试 APK；
macOS `.app`/`.dmg` 只在 macOS 或 macOS CI 上产出（Apple 工具链限制）。

## 目录结构

```
HongshengFeishu-Tauri/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json          # 产品名 / Bundle ID / 窗口配置
│   ├── capabilities/default.json
│   ├── src/
│   │   ├── lib.rs               # 命令与入口（托盘 / 窗口 / 插件）
│   │   ├── state.rs             # 全局状态与设置
│   │   ├── persist.rs           # 聊天记录持久化（messages.json）
│   │   └── network/
│   │       ├── protocol.rs      # HSFP v2 协议 + 分帧 + 口令加密
│   │       ├── discovery.rs     # UDP 广播发现
│   │       └── transfer.rs      # TCP 消息/文件传输 + 回执
├── frontend/
│   ├── index.html / main.js     # 单窗口 SPA（桌面双栏 + 移动端四 Tab）
│   ├── chat.js                  # 聊天组件（桌面嵌入主区 / 移动端覆盖层共用）
│   ├── common.js                # 共享工具（invoke/事件/弹窗/Toast/音效）
│   ├── styles.css               # 设计系统（飞鸽传书风格）
│   └── assets/                  # 提示音
├── tools/make_icon.py           # 图标生成
└── .github/workflows/release.yml
```

## 协议端口

- 发现：UDP `24680` 广播
- 传输：TCP 系统分配临时端口（随广播通告，避免固定端口冲突）

首次运行 macOS 会请求**本地网络权限**，请点「允许」，否则发现不了设备。
