import { LifeBuoy } from 'lucide-react'

import { Button } from '@/components/Button'
import { EnvironmentChecks, EnvironmentProgress } from '@/components/Environment'
import { LogConsole } from '@/components/LogConsole'
import { PaneHeader } from '@/components/PaneHeader'
import { ThinkingOrb } from '@/components/ThinkingOrb'
import { t } from '@/lib/i18n'
import { labelOf, toneOf } from '@/lib/status'
import { useHarness } from '@/state/harness'

/**
 * The console: everything the supervised runtime needs from a human.
 *
 * The status head answers "is it running" at a glance and keeps Start/Stop one
 * click away; the environment checklist explains whatever the answer is; the
 * log is the raw truth underneath. Nothing here duplicates a manager — this is
 * the one surface that speaks for the process itself.
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
                <Button variant="secondary" size="sm" disabled={busy} onClick={() => void stop()}>
                  {t('action.stop')}
                </Button>
              ) : (
                <Button size="sm" disabled={busy} onClick={() => void start()}>
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

        <EnvironmentProgress />
        <EnvironmentChecks />

        <section className="flex min-h-0 flex-1 flex-col">
          <div className="mb-2 flex items-center gap-2">
            <LifeBuoy size={13} className="text-faint" aria-hidden="true" />
            <h3 className="caption flex-1">{t('log.title')}</h3>
          </div>
          <div className="h-72 overflow-hidden rounded-panel border border-line">
            <LogConsole lines={lines} onClear={clear} />
          </div>
        </section>
      </div>
    </div>
  )
}
