import { useCallback, useState } from 'react'
import { LifeBuoy, ArrowUpCircle, FileOutput, ClipboardCopy } from 'lucide-react'
import { openUrl, revealItemInDir } from '@tauri-apps/plugin-opener'

import { Button } from '@/components/Button'
import { EnvironmentChecks, EnvironmentProgress } from '@/components/Environment'
import { LogConsole } from '@/components/LogConsole'
import { PaneHeader } from '@/components/PaneHeader'
import { ThinkingOrb } from '@/components/ThinkingOrb'
import { t } from '@/lib/i18n'
import { labelOf, toneOf } from '@/lib/status'
import { notesForDisplay } from '@/lib/updater'
import * as ipc from '@/lib/ipc'
import { useHarness } from '@/state/harness'
import { useUpdate } from '@/state/update'

/**
 * The console: everything the supervised runtime needs from a human.
 *
 * The status head answers "is it running" at a glance and keeps Start/Stop one
 * click away; the environment checklist explains whatever the answer is; the
 * log is the raw truth underneath. When a release is waiting, the update card
 * sits above all of it — the one thing this surface shows that the harness
 * itself cannot.
 */
export function Dashboard() {
  const status = useHarness((state) => state.status)
  const lines = useHarness((state) => state.lines)
  const clear = useHarness((state) => state.clear)
  const start = useHarness((state) => state.start)
  const stop = useHarness((state) => state.stop)
  const busy = useHarness((state) => state.busy)
  const error = useHarness((state) => state.error)

  const phase = status.phase
  const running = phase === 'ready'

  return (
    <div className="flex min-h-full flex-col">
      <PaneHeader title={t('nav.console')} />

      <div className="flex flex-col gap-4 px-4 py-4">
        <UpdateCard />
        <StatusCard
          status={status}
          running={running}
          busy={busy}
          error={error}
          onStart={() => void start()}
          onStop={() => void stop()}
        />
        <EnvironmentProgress />
        <EnvironmentChecks />
        <LogSection lines={lines} onClear={clear} />
      </div>
    </div>
  )
}

/** Start/Stop and the phase it reports, with the origin it serves. */
function StatusCard({
  status,
  running,
  busy,
  error,
  onStart,
  onStop,
}: {
  status: ReturnType<typeof useHarness.getState>['status']
  running: boolean
  busy: boolean
  error: string | null
  onStart: () => void
  onStop: () => void
}) {
  const phase = status.phase
  return (
    <section className="rounded-panel border border-line bg-canvas-deep/40 p-4">
      <div className="flex items-center gap-2.5">
        <ThinkingOrb tone={toneOf(status)} size={18} />
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-semibold text-text">{labelOf(status)}</div>
          {phase === 'ready' && 'origin' in status && (
            <div className="selectable mt-0.5 truncate text-[11px] text-faint">
              {status.origin}
            </div>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {running ? (
            <Button variant="secondary" size="sm" disabled={busy} onClick={onStop}>
              {t('action.stop')}
            </Button>
          ) : (
            <Button size="sm" disabled={busy} onClick={onStart}>
              {phase === 'failed' ? t('action.retry') : t('action.start')}
            </Button>
          )}
        </div>
      </div>
      {error && (
        <p className="selectable mt-3 rounded-control border border-danger/30 bg-danger/10 px-2.5 py-2 text-[11.5px] text-danger">
          {error}
        </p>
      )}
    </section>
  )
}

/**
 * A release waiting to be installed, shown while it waits.
 *
 * Everything here reads the carried update store; the card only exists when
 * there is something to say — an installed-latest window sees nothing extra.
 */
function UpdateCard() {
  const release = useUpdate((state) => state.release)
  const installing = useUpdate((state) => state.installing)
  const progress = useUpdate((state) => state.progress)
  const error = useUpdate((state) => state.error)
  const install = useUpdate((state) => state.install)

  if (!release) return null
  const percent =
    progress && progress.total != null && progress.total > 0
      ? Math.min(100, Math.round((progress.downloaded / progress.total) * 100))
      : null

  return (
    <section className="rounded-panel border border-brand/30 bg-brand/8 p-4">
      <div className="flex items-center gap-2.5">
        <ArrowUpCircle size={18} className="shrink-0 text-brand" aria-hidden="true" />
        <div className="min-w-0 flex-1">
          <div className="text-[13px] font-semibold text-text">
            {t('about.available', { version: release.version })}
          </div>
          {notesForDisplay(release.notes) && (
            <p className="selectable mt-0.5 line-clamp-2 text-[11.5px] text-muted">
              {notesForDisplay(release.notes)}
            </p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void openUrl(release.url)}
          >
            {t('about.release')}
          </Button>
          <Button size="sm" disabled={installing} onClick={() => void install()}>
            {installing ? t('about.installing') : t('about.install')}
          </Button>
        </div>
      </div>
      {percent !== null && installing && (
        <div className="mt-3">
          <div className="h-1 overflow-hidden rounded-full bg-surface-2">
            <div
              className="h-full rounded-full"
              style={{ width: `${percent}%`, background: 'var(--gradient-accent)' }}
            />
          </div>
          <p className="mt-1.5 text-[11px] text-faint">
            {t('about.downloadingPercent', { percent })}
          </p>
        </div>
      )}
      {error && (
        <p className="selectable mt-3 rounded-control border border-danger/30 bg-danger/10 px-2.5 py-2 text-[11.5px] text-danger">
          {error}
        </p>
      )}
    </section>
  )
}

/** The harness output, with the diagnostics export one click away. */
function LogSection({ lines, onClear }: { lines: ipc.LogLine[]; onClear: () => void }) {
  const [building, setBuilding] = useState(false)

  const build = useCallback(async () => {
    setBuilding(true)
    try {
      return await ipc.reportBuild()
    } finally {
      setBuilding(false)
    }
  }, [])

  const copyReport = useCallback(async () => {
    const report = await build()
    await navigator.clipboard.writeText(report.text)
  }, [build])

  const saveArchive = useCallback(async () => {
    const report = await build()
    const { save } = await import('@tauri-apps/plugin-dialog')
    const path = await save({
      title: t('about.reportTitle'),
      defaultPath: report.archiveName,
      filters: [{ name: t('about.reportKind'), extensions: ['zip'] }],
    })
    if (!path) return
    await ipc.reportArchive(path, report.text)
    await revealItemInDir(path)
  }, [build])

  return (
    <section className="flex min-h-0 flex-1 flex-col">
      <div className="mb-2 flex items-center gap-2">
        <LifeBuoy size={13} className="text-faint" aria-hidden="true" />
        <h3 className="caption flex-1">{t('log.title')}</h3>
        <Button
          variant="ghost"
          size="sm"
          disabled={building}
          title={t('about.reportHint')}
          onClick={() => void copyReport()}
        >
          <ClipboardCopy size={11} aria-hidden="true" />
          {t('about.reportCopy')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          disabled={building}
          title={t('about.diagnosticsBody')}
          onClick={() => void saveArchive()}
        >
          <FileOutput size={11} aria-hidden="true" />
          {t('about.reportSave')}
        </Button>
      </div>
      <div className="h-72 overflow-hidden rounded-panel border border-line">
        <LogConsole lines={lines} onClear={onClear} />
      </div>
    </section>
  )
}
