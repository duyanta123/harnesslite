/**
 * Browser-mode IPC mock, for layout self-checks outside the desktop shell.
 *
 * Installed by `main.tsx` only when the page is NOT running inside Tauri (no
 * `__TAURI_INTERNALS__`), which is exactly the "self-check in a browser" path
 * the development plan calls for. The mock answers rosters with empty truth and
 * lifecycle actions with honest refusals, so every pane renders without a Rust
 * process behind it; it never ships to the desktop, where real IPC wins by
 * existing.
 */
import type { Environment } from '@/lib/ipc'

const fixtureEnvironment: Environment = {
  node: {
    path: 'C:\\Program Files\\nodejs\\node.exe',
    version: { major: 22, minor: 21, patch: 0 },
    source: 'system',
  },
  allNodeRuntimes: [],
  minimumNode: { major: 22, minor: 19, patch: 0 },
  harnessInstalled: true,
  harnessCompatible: true,
  harnessVersion: '0.1.1-rc.2',
  expectedHarnessVersion: '0.1.1-rc.2',
  harnessProblem: null,
  harnessEntry:
    'C:\\Users\\you\\AppData\\Local\\harnesslite\\harness\\node_modules\\@deepseek-ai\\dsh\\lib\\bin.js',
  project: 'deep seek harness',
  workspace: 'D:\\deep seek harness',
  workspaceAdmission: { state: 'safe', filesystem: 'NTFS', reason: null },
}

/** Rosters the stores read on mount; empty is the shape every pane renders. */
const EMPTY_ROSTER: Record<string, unknown> = {
  harness_status: { phase: 'stopped' },
  projects_list: { selected: '', projects: [] },
  profile_roster: { profiles: [], selected: '', root: 'C:\\Users\\you\\.dsh\\profiles' },
  session_roster: { cards: [], loaded: 0 },
  plugin_state: {
    profile: 'web',
    profileDir: 'C:\\Users\\you\\.dsh\\profiles\\web',
    initialized: true,
    plugins: [],
    packageManager: true,
  },
  remote_status: {
    open: false,
    addresses: [],
    url: null,
    pairingUrl: null,
    qr: null,
    codeSecondsLeft: null,
    codeLifetimeSeconds: 120,
    devices: [],
    active: 0,
    served: 0,
    refused: 0,
  },
  preset_roster: { presets: [], default: null },
  startup_state: {
    autostart: false,
    shortcut: null,
    held: false,
    suggested: 'CmdOrCtrl+Shift+D',
    notifications: { turnCompleted: true, turnFailed: true, jobCompleted: false, jobFailed: true },
    logLevel: 'info',
    harnessPort: null,
  },
  terminal_list: [],
  harness_log: [],
  node_selection: null,
  plugin_search: {
    items: [],
    categories: [],
    total: 0,
    page: 0,
    pageSize: 25,
    hasMore: false,
    indexedAt: 0,
  },
  plugin_sources: [
    {
      id: 'npm',
      label: 'npm',
      kind: 'npm',
      endpoint: null,
      builtIn: true,
      active: true,
    },
  ],
  plugin_recovery_notice: null,
  profile_recovery_notice: null,
  recovery_state: null,
  update_state: null,
}

const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
  harness_environment: () => fixtureEnvironment,
  ...Object.fromEntries(
    Object.entries(EMPTY_ROSTER).map(([command, value]) => [command, () => value]),
  ),
  // Lifecycle actions are real work; the mock refuses honestly so failure
  // surfaces can be checked too.
  harness_install: () => {
    throw new Error('browser self-check: the install transaction needs the desktop shell')
  },
  harness_start: () => {
    throw new Error('browser self-check: the harness does not run outside the desktop shell')
  },
  node_provision: () => {
    throw new Error('browser self-check: the Node download needs the desktop shell')
  },
  harness_stop: () => undefined,
  renderer_ready: () => undefined,
  desktop_badge: () => undefined,
  desktop_attention: () => undefined,
  desktop_notify: () => undefined,
  desktop_offer: () => ({
    protocol: 3,
    app: 'HarnessLite',
    version: '0.1.0',
    platform: 'browser',
    scheme: 'harnesslite',
    capabilities: [],
    link: null,
  }),
}

export function installIpcMock(): void {
  const internals = window as unknown as {
    __TAURI_INTERNALS__?: {
      invoke: (cmd: string, args?: unknown) => Promise<unknown>
      metadata?: Record<string, unknown>
    }
  }
  internals.__TAURI_INTERNALS__ = {
    // `@tauri-apps/api/window` reads the current window label out of here; a
    // browser self-check pretends to be the main window so the title bar and
    // the drag-drop listener construct at all.
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main' } },
    invoke: (cmd: string, args?: unknown) => {
      const handler = handlers[cmd]
      if (handler) return Promise.resolve(handler((args ?? {}) as Record<string, unknown>))
      // Unrecognised commands are rejected — refusal, not silence, is what a
      // self-check is for.
      return Promise.reject(new Error(`browser mock: no handler for ${cmd}`))
    },
  }
}
