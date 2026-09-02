//! The Desktop Protocol — the single point where every cross-boundary
//! constant lives.
//!
//! Three physical channels connect the shell to the framed Harness web UI:
//! environment variables injected by the supervisor, a `postMessage` bridge
//! between the window and the iframe, and a read-only Host service exposed by
//! the cordis patch package. Each channel is necessary; what changed from the
//! first generation of this product is that none of the constants are
//! hand-written anywhere else. The supervisor reads them from here, the codegen
//! step writes the frontend mirror from here, and contract tests pin all three
//! sides together.
//!
//! When the upstream `@deepseek-ai/dsh` changes how it boots or names its
//! knobs, this file is the whole review surface.

/// The managed npm package, and the exact version this shell locks.
pub const DSH_PACKAGE: &str = "@deepseek-ai/dsh";
pub const DSH_VERSION: &str = "0.1.1-rc.2";

/// The npm private package injected with `--patch` at boot; it exposes the
/// read-only Host service to the harness process.
pub const INTEGRATION_PACKAGE: &str = "@duyanta123/harnesslite-integration";

// --- channel 1: environment variables --------------------------------------

/// Version of the desktop shell, read by the integration package.
pub const ENV_VERSION: &str = "HARNESSLITE_VERSION";
/// Version of the installed `@deepseek-ai/dsh` runtime.
pub const ENV_RUNTIME_VERSION: &str = "HARNESSLITE_RUNTIME_VERSION";
/// Profile name the harness process was started with.
pub const ENV_PROFILE: &str = "HARNESSLITE_PROFILE";
/// Absolute path of that profile's directory.
pub const ENV_PROFILE_DIR: &str = "HARNESSLITE_PROFILE_DIR";

/// Marker the harness and its tools use to detect a desktop shell.
pub const ENV_DESKTOP: &str = "DSH_DESKTOP";
pub const ENV_DESKTOP_VALUE: &str = "1";
/// Harness-owned variables the supervisor passes through verbatim.
pub const ENV_DSH_PROFILE: &str = "DSH_PROFILE";
pub const ENV_DSH_PROFILE_DIR: &str = "DSH_PROFILE_DIR";
pub const ENV_DSH_HOME: &str = "DSH_HOME";

// --- channel 2: the postMessage bridge -------------------------------------

/// Bridge protocol version. Negotiated in `hello`; a frame that does not speak
/// this exact version is left alone.
pub const BRIDGE_PROTOCOL: u32 = 3;

/// The methods the framed UI may call, in the order `hello` reports them.
pub const BRIDGE_METHODS: [&str; 10] = [
    "hello",
    "notify",
    "attention",
    "pick",
    "workspace.validate",
    "badge",
    "profiles.list",
    "profiles.select",
    "plugins.install",
    "plugins.remove",
];

// --- channel 3: the Host service -------------------------------------------

/// Protocol number the integration package reports in its Host service.
pub const HOST_PROTOCOL: u32 = 1;
/// Name of the read-only cordis service the integration package exposes.
pub const HOST_SERVICE: &str = "harnessLiteHost";

// --- readiness -------------------------------------------------------------

/// The stdout line prefix the harness prints when its web UI is up:
/// `dsh web: http://127.0.0.1:<port>`.
pub const READY_LINE_PREFIX: &str = "dsh web: ";

// --- shell event channels --------------------------------------------------

pub const EVENT_HARNESS: &str = "harnesslite://harness";
pub const EVENT_REMOTE: &str = "harnesslite://remote";
pub const EVENT_TERMINAL_OUTPUT: &str = "harnesslite://terminal/output";
pub const EVENT_TERMINAL_EXIT: &str = "harnesslite://terminal/exit";
pub const EVENT_NODE_PROGRESS: &str = "harnesslite://node/progress";
pub const EVENT_SHARED_STATE: &str = "harnesslite://announce";
pub const EVENT_DESKTOP_LINK: &str = "harnesslite://desktop/link";

/// Deep-link scheme handled by the desktop shell.
pub const DEEP_LINK_SCHEME: &str = "harnesslite";

/// localStorage key prefix owned by the frontend.
pub const STORAGE_PREFIX: &str = "harnesslite.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bridge_version_and_methods_are_the_negotiated_set() {
        assert_eq!(BRIDGE_PROTOCOL, 3);
        assert_eq!(BRIDGE_METHODS[0], "hello");
        assert_eq!(BRIDGE_METHODS.len(), 10);
    }

    #[test]
    fn the_runtime_lock_is_explicit() {
        assert_eq!(DSH_PACKAGE, "@deepseek-ai/dsh");
        assert_eq!(DSH_VERSION, "0.1.1-rc.2");
    }

    #[test]
    fn event_channels_live_under_one_scheme() {
        for channel in [
            EVENT_HARNESS,
            EVENT_REMOTE,
            EVENT_TERMINAL_OUTPUT,
            EVENT_TERMINAL_EXIT,
            EVENT_NODE_PROGRESS,
            EVENT_SHARED_STATE,
            EVENT_DESKTOP_LINK,
        ] {
            assert!(channel.starts_with("harnesslite://"), "{channel}");
        }
    }
}
