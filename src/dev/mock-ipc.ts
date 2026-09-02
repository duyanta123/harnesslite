/**
 * Browser-mode IPC mock, for layout self-checks outside the desktop shell.
 *
 * Installed by `main.tsx` only when the page is NOT running inside Tauri (no
 * `__TAURI_INTERNALS__`), which is exactly the "self-check in a browser" path
 * the development plan calls for. The mock answers with fixture data so the
 * loader and its states render without a Rust process behind them; it never
 * ships to the desktop, where real IPC wins by existing.
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
  harnessInstalled: false,
  harnessCompatible: false,
  harnessVersion: null,
  expectedHarnessVersion: '0.1.1-rc.2',
  harnessProblem: null,
  harnessEntry: 'C:\\Users\\you\\AppData\\Local\\harnesslite\\harness\\node_modules\\@deepseek-ai\\dsh\\lib\\bin.js',
  project: 'deep seek harness',
  workspace: 'D:\\deep seek harness',
  workspaceAdmission: { state: 'safe', filesystem: 'NTFS', reason: null },
}

const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
  harness_environment: () => fixtureEnvironment,
  harness_status: () => ({ phase: 'stopped' }),
  harness_log: () => [],
  // An install is a real npm run; the mock refuses honestly so the failure
  // screen can be checked too.
  harness_install: () => {
    throw new Error('the install transaction is not wired into this build yet')
  },
  harness_start: () => {
    throw new Error('browser self-check: the harness does not run outside the desktop shell')
  },
  harness_stop: () => undefined,
  desktop_offer: () => ({
    protocol: 3,
    app: 'HarnessLite',
    version: '0.1.0',
    platform: 'browser',
    scheme: 'harnesslite',
    capabilities: [],
    link: null,
  }),
  renderer_ready: () => undefined,
  desktop_badge: () => undefined,
  desktop_attention: () => undefined,
  desktop_notify: () => undefined,
}

export function installIpcMock(): void {
  const internals = window as unknown as {
    __TAURI_INTERNALS__?: { invoke: (cmd: string, args?: unknown) => Promise<unknown> }
  }
  internals.__TAURI_INTERNALS__ = {
    invoke: (cmd: string, args?: unknown) => {
      const handler = handlers[cmd]
      if (!handler) {
        return Promise.reject(new Error(`browser mock: no handler for ${cmd}`))
      }
      return Promise.resolve(handler((args ?? {}) as Record<string, unknown>))
    },
  }
}
