# 鸿升飞书（Tauri 版）

局域网即时通讯与文件传输工具：自动发现网段内同事、点对点收发消息与文件，**无需服务器**。
界面与功能对标飞鸽传书，但代码、图标、协议均为**原创重写**，版权归属**鸿升周老师**。

> 这是用 **Tauri v2**（Rust 后端 + Web 前端）重写的版本，相比早期 Swift 版更易跨平台打包分发。

## 功能

- 局域网 UDP 广播自动发现，在线/离线实时更新
- 点对点文本聊天（TCP 直连）
- 文件 / 文件夹传输（文件夹自动打包为 zip 分片发送，带进度）
- 拖拽即传、接收进度条
- 聊天记录本地存储（收件箱在应用数据目录）

## 技术栈

- 后端：Rust（零第三方网络库，自研 HSFP 协议）
- 前端：原生 HTML / CSS / JS（Tauri 全局 API 通信）
- 打包：Tauri v2 + `tauri-action` 云构建

## 三种方式拿到安装包

### 1. 自己的 Mac（推荐，一条命令）

```bash
cd HongshengFeishu-Tauri/src-tauri
cargo install tauri-cli --version "^2"   # 首次需装 CLI
cargo tauri build
# 产物：target/release/bundle/dmg/鸿升飞书_1.0.0_aarch64.dmg
#       target/release/bundle/macos/鸿升飞书.app
```

### 2. 没有 Mac 也能出真包（GitHub Actions 云构建，推荐）

`release.yml` 用官方 `tauri-action` 在 GitHub 自带的 **macOS 运行器** 上构建，自动产出原生
`.app` 与 `.dmg`（运行器为 Apple Silicon，产物为 `aarch64`；需 Intel 版可改 `args: --target x86_64-apple-darwin`）。

```bash
# 在工程根目录（含 .github/workflows/release.yml）执行
git add -A && git commit -m "init"
git push -u origin main
git tag v1.0.0 && git push --tags
# 到 GitHub → 该仓库 → Actions → 自动跑「Build macOS」→ 完成后在 Releases 下载
# 鸿升飞书_1.0.0_aarch64.dmg 与 鸿升飞书.app
```

> 也可不打标签：仓库页 → Actions → 选「Build macOS」→ Run workflow 手动触发。

如需对外无提示分发，在仓库 **Settings → Secrets** 填入
`APPLE_CERTIFICATE` / `APPLE_CERTIFICATE_PASSWORD` / `APPLE_SIGNING_IDENTITY` /
`APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID`，即可自动 **Developer ID 签名 + 公证**。

### 3. 沙箱里我能验证什么

本仓库在 Linux 沙箱中可 `cargo build` 通过，证明 Rust 工程可编译、协议可运行；
**真正的 macOS `.app`/`.dmg` 只在 macOS 或 macOS CI 上产出**（Apple 工具链限制，非 Tauri 特有）。
无 Mac 时，方案 2 的 GitHub Actions 就是「云上 macOS」——把代码推上去，包自动生成。

## 目录结构

```
HongshengFeishu-Tauri/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json          # 产品名/ Bundle ID / DMG 目标
│   ├── capabilities/default.json
│   ├── src/
│   │   ├── lib.rs               # 命令与入口
│   │   ├── state.rs             # 全局状态
│   │   └── network/
│   │       ├── protocol.rs      # HSFP 协议 + 分帧
│   │       ├── discovery.rs     # UDP 广播发现
│   │       └── transfer.rs      # TCP 消息/文件传输
├── frontend/                    # 侧边栏 + 聊天区界面
├── tools/make_icon.py           # 图标生成
└── .github/workflows/release.yml
```

## 协议端口

- 发现：UDP `24680` 广播
- 传输：TCP `24681`

首次运行 macOS 会请求**本地网络权限**，请点「允许」，否则发现不了设备。
