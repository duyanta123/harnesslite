# HarnessLite

**HarnessLite** 是 DeepSeek Harness 官方 web 界面（`dsh web`）的 Windows 原生桌面壳：双击启动，自动安装并托管官方 `@deepseek-ai/dsh` 运行时，在桌面窗口内呈现官方 web UI，并自带项目管理、Profile 管理、插件市场、内置终端、会话库、远程访问等完整管理面。

**HarnessLite** is a native desktop shell for the DeepSeek Harness. One double-click: the shell installs and supervises the official `@deepseek-ai/dsh` web service locally, embeds its UI in a desktop window, and adds a full management plane — projects, profiles, a plugin market, a real terminal, the session library and LAN remote access.

> HarnessLite is an independent community project and is not affiliated with DeepSeek.

## Features / 功能

- **对话面一等公民**：官方 Harness web UI 全高内嵌；管理面从右缘滑出，不打断会话
- **运行时全自动**：首次运行自动下载 Node LTS（官方源 / npmmirror 镜像）并安装锁定的 `@deepseek-ai/dsh`（staging → 原子晋升 → 备份回滚的就绪事务）；崩溃恢复自动进行
- **项目管理**：本地文件夹 + Profile 绑定，路径安全校验，切换即重启
- **Profile 管理**：`~/.dsh/profiles` 编排，CRUD/复制/对比/声明导出导入，lastKnownGood 故障恢复
- **插件市场**：npm + awesome-dsh + DSH 1024Store + 自定义源；npm 隔离预检 → 意图令牌确认 → 官方 CLI 安装；一键停用（运行时 patch 生效）
- **内置终端**：ConPTY 真终端多标签，`dsh`/git/npm 环境自动配好，cwd 跟随项目，独立进程回收
- **会话库**：直读 `~/.dsh/sessions`（Zstd JSONL），全文搜索、导出 Markdown/HTML/JSON、费用报表
- **远程访问**：LAN 反向代理 + QR 配对（一次性码、2 分钟有效），每设备可吊销凭据
- **壳**：托盘常驻、单实例、全局快捷键（Ctrl+Shift+D 唤出）、自启动、Ctrl+K 命令面板、中英双语、深浅主题、诊断导出（脱敏）

## Download / 下载

从 [GitHub Releases](https://github.com/duyanta123/harnesslite/releases) 下载：

- `HarnessLite_0.1.1_x64-setup.exe` — NSIS 安装包（推荐）
- `harnesslite.exe` — 免安装绿色版

首次运行需联网安装 Harness 运行时（内置 Node 下载 + 一次 npm 安装），之后离线可用。未签名构建首次运行会有 SmartScreen 提示：**更多信息 → 仍要运行**。

## Development / 开发

```powershell
pnpm install
pnpm dev              # Vite dev server only (browser check)
pnpm tauri dev        # full desktop shell
pnpm test             # vitest
pnpm tauri build      # NSIS installer
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

架构：React 19 + Zustand + Tailwind 4 前端；薄 Tauri 命令层；Rust 分层 `hd-core`（纯数据域）与 `hd-runtime`（进程/网络生命周期）+ `proc-guard` / `node-runtime`。集成合同（env 变量、桥协议、注入包）收敛在 `hd-core::contract` 单点。发布构建把 Node 运行时打进安装包（`scripts/prepare-runtime.mjs` + `tauri.full.conf.json`），无 Node 的机器开箱即用。

If the network blocks npm / nodejs.org, set session-scope proxy env vars before retrying:

```powershell
$env:HTTP_PROXY="http://127.0.0.1:7877"
$env:HTTPS_PROXY="http://127.0.0.1:7877"
```

## License

MIT — see [LICENSE](LICENSE). Portions of the build/packaging infrastructure are derived from dsh-studio (MIT).
