# Desktop Protocol / 桌面协议

**English**
The Desktop Protocol is the complete contract between the HarnessLite desktop shell and the framed DeepSeek Harness web UI. It has three physical channels; every constant on all three is defined once, in `src-tauri/crates/hd-core/src/contract.rs`, and mirrored into the frontend by `src/console/protocol.ts`. CI pins the two (`pnpm verify:contracts`), so an upstream `@deepseek-ai/dsh` upgrade is reviewed against one file and one page — not a repo-wide grep.

1. **Environment variables** — injected by the supervisor into the harness process: `HARNESSLITE_VERSION`, `HARNESSLITE_RUNTIME_VERSION`, `HARNESSLITE_PROFILE`, `HARNESSLITE_PROFILE_DIR`, plus the pass-through `DSH_DESKTOP=1`, `DSH_PROFILE`, `DSH_PROFILE_DIR`, `DSH_HOME`.
2. **The postMessage bridge** — the framed UI calls shell capabilities through a letterbox (`src/lib/bridge.ts`). Protocol version **3**; methods: `hello`, `notify`, `attention`, `pick`, `workspace.validate`, `badge`, `profiles.list`, `profiles.select`, `plugins.install`, `plugins.remove`. A request is served only when its sender's origin is the origin the harness is serving **and** the sender is a frame of this window.
3. **The Host service** — the `--patch`-injected cordis package `@duyanta123/harnesslite-integration` exposes a read-only service named `harnessLiteHost`, Host Protocol **1**, built only from launcher-authenticated environment values. Plugins never receive a native handle, arbitrary command runner, or package-manager authority.

Readiness is announced on stdout as `dsh web: http://127.0.0.1:<port>`; the supervisor accepts only explicit-port loopback HTTP origins. The shell never persists its integration patch into a user profile bundle stack — it is supplied per process.

**中文**
桌面协议是 HarnessLite 桌面壳与内嵌的 DeepSeek Harness web UI 之间的完整合同。它有三条物理通道；三者的所有常量都只在 `src-tauri/crates/hd-core/src/contract.rs` 定义一次，并由 `src/console/protocol.ts` 镜像到前端。CI 用 `pnpm verify:contracts` 钉住两侧——上游 `@deepseek-ai/dsh` 升级时的检查面是一个文件加这一页，而不是全仓 grep。

1. **环境变量** —— 由 supervisor 注入 harness 进程：`HARNESSLITE_VERSION`、`HARNESSLITE_RUNTIME_VERSION`、`HARNESSLITE_PROFILE`、`HARNESSLITE_PROFILE_DIR`，以及透传的 `DSH_DESKTOP=1`、`DSH_PROFILE`、`DSH_PROFILE_DIR`、`DSH_HOME`。
2. **postMessage 桥** —— 内嵌 UI 通过信箱（`src/lib/bridge.ts`）调用壳能力。协议版本 **3**；方法：`hello`、`notify`、`attention`、`pick`、`workspace.validate`、`badge`、`profiles.list`、`profiles.select`、`plugins.install`、`plugins.remove`。仅当发送方 origin 是 harness 当前服务的 origin **且**发送方是本窗口的 frame 时才应答。
3. **Host 服务** —— 以 `--patch` 注入的 cordis 包 `@duyanta123/harnesslite-integration` 暴露只读服务 `harnessLiteHost`，Host 协议 **1**，全部由启动器认证的环境值构建。插件永远拿不到原生句柄、任意命令执行器或包管理器权限。

就绪通告在 stdout 上以 `dsh web: http://127.0.0.1:<port>` 形式出现；supervisor 只接受带显式端口的回环 HTTP origin。壳不会把集成补丁持久化进用户 profile 的 bundle 栈——它按进程提供。
