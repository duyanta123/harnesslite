/**
 * The frontend mirror of the Desktop Protocol.
 *
 * Every constant here is owned by `src-tauri/crates/hd-core/src/contract.rs`;
 * this file is generated from it and pinned by `scripts/verify-contract-pin.mjs`,
 * which fails CI if the two ever stop agreeing. Edit the Rust file, then
 * regenerate this one — never the other way around.
 */

export const DSH_PACKAGE = '@deepseek-ai/dsh'
export const DSH_VERSION = '0.1.1-rc.2'
export const INTEGRATION_PACKAGE = '@duyanta123/harnesslite-integration'

export const ENV_VERSION = 'HARNESSLITE_VERSION'
export const ENV_RUNTIME_VERSION = 'HARNESSLITE_RUNTIME_VERSION'
export const ENV_PROFILE = 'HARNESSLITE_PROFILE'
export const ENV_PROFILE_DIR = 'HARNESSLITE_PROFILE_DIR'
export const ENV_DESKTOP = 'DSH_DESKTOP'
export const ENV_DESKTOP_VALUE = '1'
export const ENV_DSH_PROFILE = 'DSH_PROFILE'
export const ENV_DSH_PROFILE_DIR = 'DSH_PROFILE_DIR'
export const ENV_DSH_HOME = 'DSH_HOME'

export const READY_LINE_PREFIX = 'dsh web: '

export const BRIDGE_PROTOCOL = 3
export const BRIDGE_METHODS = [
  'hello',
  'notify',
  'attention',
  'pick',
  'workspace.validate',
  'badge',
  'profiles.list',
  'profiles.select',
  'plugins.install',
  'plugins.remove',
] as const

export const HOST_PROTOCOL = 1
export const HOST_SERVICE = 'harnessLiteHost'

export const EVENT_HARNESS = 'harnesslite://harness'
export const EVENT_REMOTE = 'harnesslite://remote'
export const EVENT_TERMINAL_OUTPUT = 'harnesslite://terminal/output'
export const EVENT_TERMINAL_EXIT = 'harnesslite://terminal/exit'
export const EVENT_NODE_PROGRESS = 'harnesslite://node/progress'
export const EVENT_SHARED_STATE = 'harnesslite://announce'
export const EVENT_DESKTOP_LINK = 'harnesslite://desktop/link'

export const DEEP_LINK_SCHEME = 'harnesslite'
export const STORAGE_PREFIX = 'harnesslite.'
