import { lazy, Suspense, useCallback, useEffect, useMemo, useState } from 'react'
import {
  LifeBuoy,
  PlugZap,
  Settings as SettingsIcon,
  SquareTerminal,
  TerminalSquare,
  Users,
} from 'lucide-react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import type { LucideIcon } from 'lucide-react'

import { ContextMenu } from '@/components/ContextMenu'
import { Dialog } from '@/components/Dialog'
import { HarnessFrame } from '@/console/HarnessFrame'
import { Onboarding } from '@/components/Onboarding'
import { RecoveryCenter } from '@/components/RecoveryCenter'
import { StatusBar } from '@/components/StatusBar'
import { Tooltip } from '@/components/Tooltip'
import { t } from '@/lib/i18n'
import { pushWorkspaceDrop } from '@/lib/bridge'
import * as ipc from '@/lib/ipc'
import { standby } from '@/lib/platform'
import { addProjectWorkspace } from '@/state/projects'
import { useDialog } from '@/state/dialog'
import { subscribeToHarness, useHarness } from '@/state/harness'
import { useOnboarding } from '@/state/onboarding'
import { usePalette } from '@/state/palette'
import { subscribeToProfiles } from '@/state/profiles'
import { subscribeToProjects } from '@/state/projects'
import { subscribeToRemote, useRemote } from '@/state/remote'
import { watchForUpdates } from '@/state/update'
import { Sheet } from '@/shell/Sheet'
import { TitleBar } from '@/shell/TitleBar'
import { Dashboard } from '@/shell/Dashboard'
import { Omni, type OmniCommand } from '@/shell/Omni'

const ProfileManager = lazy(() =>
  import('@/components/ProfileManager').then((module) => ({ default: module.ProfileManager })),
)
const TerminalPane = lazy(() =>
  import('@/components/TerminalPane').then((module) => ({ default: module.TerminalPane })),
)
const SessionsPane = lazy(() =>
  import('@/components/SessionsPane').then((module) => ({ default: module.SessionsPane })),
)
const PluginMarket = lazy(() =>
  import('@/components/PluginMarket').then((module) => ({ default: module.PluginMarket })),
)
const RemotePane = lazy(() =>
  import('@/components/RemotePane').then((module) => ({ default: module.RemotePane })),
)
const SettingsPane = lazy(() =>
  import('@/components/SettingsPane').then((module) => ({ default: module.SettingsPane })),
)

/** The surfaces a sheet can hold. `null` is the conversation filling the window. */
type SheetId = 'console' | 'terminal' | 'sessions' | 'plugins' | 'remote' | 'settings'

const SHEETS: Array<{ id: SheetId; icon: LucideIcon }> = [
  { id: 'console', icon: LifeBuoy },
  { id: 'terminal', icon: SquareTerminal },
  { id: 'sessions', icon: Users },
  { id: 'plugins', icon: PlugZap },
  { id: 'remote', icon: TerminalSquare },
  { id: 'settings', icon: SettingsIcon },
]

/**
 * The window: a title bar, a status bar, and the conversation between them.
 *
 * The harness iframe is the main view and never unmounts once it exists; every
 * management surface is a visit — a sheet over the right edge that closes back
 * into the session. While nothing is serving, the console stands in as the main
 * view, because starting the runtime is then the only thing worth looking at.
 */
export default function App() {
  const status = useHarness((state) => state.status)
  const environment = useHarness((state) => state.environment)
  const inspect = useHarness((state) => state.inspect)
  const refreshRemote = useRemote((state) => state.refresh)
  const stage = useOnboarding((state) => state.stage)
  const consider = useOnboarding((state) => state.consider)
  const paletteOpen = usePalette((state) => state.open)
  const origin = status.phase === 'ready' ? status.origin : null

  const [sheet, setSheet] = useState<SheetId | null>(null)
  const [managing, setManaging] = useState(false)

  // Cancels Rust's static startup-recovery deadline once React has committed;
  // not a harness health signal and deliberately not waiting for a profile.
  useEffect(() => {
    void ipc.rendererReady().catch(() => {
      // The browser self-check has no recovery channel; the visible UI is the
      // whole story there.
    })
  }, [])

  // The one look at the machine: the console reports it and the guide exists
  // or does not because of it. Settled on both paths so the window still opens.
  useEffect(() => {
    const settle = () => consider(useHarness.getState().environment)
    void inspect().then(settle, settle)
  }, [inspect, consider])

  // Created hidden; revealed after the probe settles, so the first painted
  // frame is the guide or the conversation and never one replaced by the other.
  useEffect(() => {
    if (standby || stage === 'unknown') return
    let frame = requestAnimationFrame(() => {
      frame = requestAnimationFrame(() => {
        try {
          void getCurrentWindow().show()
        } catch {
          // The browser self-check has no window to reveal.
        }
      })
    })
    return () => cancelAnimationFrame(frame)
  }, [stage])

  useEffect(() => {
    const pending = subscribeToHarness()
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [])

  // Window-lifetime, not pane-lifetime: the supervisor closes remote access
  // when the harness stops, and the UI has to hear that wherever it is looking.
  useEffect(() => {
    void refreshRemote()
    const pending = subscribeToRemote()
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [refreshRemote])

  // Shells keep printing while nobody watches; the async import also keeps
  // xterm's emulator out of the first-paint bundle.
  useEffect(() => {
    const pending = import('@/state/terminals').then((module) => module.subscribeToTerminals())
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [])

  // One set of profiles and one project registry, seen from every window.
  useEffect(() => {
    const pending = subscribeToProfiles()
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [])
  useEffect(() => {
    const pending = subscribeToProjects()
    return () => {
      void pending.then((unlisten) => unlisten())
    }
  }, [])

  // The update schedule belongs to the window, not the strip that shows it.
  useEffect(() => watchForUpdates(), [])

  // A native drop adds the folder as a project — which switches to it when the
  // harness is idle. With a harness already serving, the drop is handed to the
  // upstream Workspace service so it becomes a session there instead.
  useEffect(() => {
    let unlisten: (() => void) | null = null
    let cancelled = false
    void getCurrentWindow()
      .onDragDropEvent((event) => {
        if (event.payload.type !== 'drop' || event.payload.paths.length !== 1) return
        const [path] = event.payload.paths
        if (!path) return
        if (origin) pushWorkspaceDrop(path, origin)
        else void addProjectWorkspace(path)
      })
      .then((stop) => {
        if (cancelled) stop()
        else unlisten = stop
      })
      .catch(() => {
        // Browser self-check: no native drop events.
      })
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [origin])

  // Ctrl+K is the keystroke for when someone knows the name of what they want.
  // Suppressed behind modals and the guide, for the same reason v1's were.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return
      if (managing || stage === 'guiding' || useDialog.getState().pending) return
      if (event.key === 'k' || event.key === 'K') {
        event.preventDefault()
        usePalette.getState().toggle()
      }
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [managing, stage])

  const manage = useCallback(() => setManaging(true), [])

  const chooseWorkspace = useCallback(async () => {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const chosen = await open({
      title: t('workspace.choose'),
      defaultPath: useHarness.getState().environment?.workspace,
      directory: true,
      multiple: false,
    })
    if (typeof chosen === 'string') await addProjectWorkspace(chosen)
  }, [])

  const openUpdate = useCallback(() => {
    setSheet('console')
    void import('@/state/update').then((module) => void module.useUpdate.getState().check(false))
  }, [])

  const commands = useMemo<OmniCommand[]>(() => {
    const entries: OmniCommand[] = SHEETS.map((entry) => ({
      id: `sheet.${entry.id}`,
      label: t(`nav.${entry.id === 'console' ? 'console' : entry.id}`),
      hint: t('palette.group.go'),
      run: () => setSheet(entry.id),
    }))
    entries.push({
      id: 'profiles.manage',
      label: t('profile.manage'),
      run: manage,
    })
    entries.push({
      id: 'workspace.choose',
      label: t('workspace.choose'),
      run: () => void chooseWorkspace(),
    })
    if (usePalette.getState().recents.length === 0) return entries
    return entries
  }, [chooseWorkspace, manage])

  const sheetMeta = SHEETS.find((entry) => entry.id === sheet) ?? null

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <TitleBar
        status={status}
        onOpenSettings={() => setSheet('settings')}
        onChooseWorkspace={() => void chooseWorkspace()}
        onOpenProfiles={manage}
        onOpenPalette={() => usePalette.getState().show()}
      />

      {stage === 'guiding' ? (
        <Onboarding />
      ) : (
        <div className="relative min-h-0 flex-1">
          {origin ? (
            <HarnessFrame origin={origin} hidden={false} />
          ) : (
            <div className="h-full overflow-y-auto">
              <Dashboard />
            </div>
          )}
        </div>
      )}

      <StatusBar
        status={status}
        environment={environment}
        onOpenUpdate={openUpdate}
        onChangeWorkspace={() => void chooseWorkspace()}
      />

      {sheetMeta && stage !== 'guiding' && (
        <Sheet
          icon={sheetMeta.icon}
          title={t(`nav.${sheetMeta.id === 'console' ? 'console' : sheetMeta.id}`)}
          onClose={() => setSheet(null)}
        >
          <Suspense fallback={<LoadingSurface />}>
            {sheet === 'console' && <Dashboard />}
            {sheet === 'terminal' && <TerminalPane />}
            {sheet === 'sessions' && <SessionsPane />}
            {sheet === 'plugins' && <PluginMarket />}
            {sheet === 'remote' && <RemotePane />}
            {sheet === 'settings' && <SettingsPane />}
          </Suspense>
        </Sheet>
      )}

      {managing && (
        <Suspense fallback={<LoadingSurface overlay />}>
          <ProfileManager onClose={() => setManaging(false)} />
        </Suspense>
      )}

      {paletteOpen && stage !== 'guiding' && <Omni commands={commands} onClose={() => usePalette.getState().hide()} />}

      {/* Last, and outside the layout: positioned against the window, so they
          can cover anything in it. The dialog before the menu, because a
          right-click inside a dialog still gets a menu. */}
      <RecoveryCenter />
      <Dialog />
      <ContextMenu />
      <Tooltip />
    </div>
  )
}

function LoadingSurface({ overlay = false }: { overlay?: boolean }) {
  return (
    <div
      role="status"
      className={[
        'grid place-items-center bg-canvas text-[12px] text-faint',
        overlay ? 'absolute inset-0 z-50' : 'min-h-0 flex-1',
      ].join(' ')}
    >
      {t('common.loading')}
    </div>
  )
}
