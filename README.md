# HarnessLite

**HarnessLite** 是 DeepSeek Harness 官方 web 界面（`dsh web`）的 Windows 原生桌面壳：双击启动，自动安装并托管官方 `@deepseek-ai/dsh` 运行时，在桌面窗口内呈现官方 web UI，并自带项目管理、Profile 管理、插件市场、内置终端、会话库、远程访问等完整管理面。

**HarnessLite** is a native desktop shell for the DeepSeek Harness. One double-click: the shell installs and supervises the official `@deepseek-ai/dsh` web service locally, embeds its UI in a desktop window, and adds a full management plane — projects, profiles, a plugin market, a real terminal, the session library and LAN remote access.

> HarnessLite is an independent community project and is not affiliated with DeepSeek.

## Status / 状态

Phase 0 scaffold — carried engineering assets (proc-guard, node-runtime, design tokens, pilot primitives) are in place; the layered Rust rewrite (hd-core / hd-runtime) and the management UI land phase by phase. See the plan in the repository history and `docs/`.

## Development / 开发

```powershell
pnpm install
pnpm dev              # Vite dev server only (browser check)
pnpm tauri dev        # full desktop shell
pnpm test             # vitest
pnpm tauri build      # NSIS installer
cargo test --manifest-path src-tauri/Cargo.toml --workspace
```

If the network blocks npm / nodejs.org, set session-scope proxy env vars before retrying:

```powershell
$env:HTTP_PROXY="http://127.0.0.1:7877"
$env:HTTPS_PROXY="http://127.0.0.1:7877"
```

## License

MIT — see [LICENSE](LICENSE). Portions of the build/packaging infrastructure are derived from dsh-studio (MIT).
