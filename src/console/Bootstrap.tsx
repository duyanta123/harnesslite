/**
 * The bootstrap surface: get a harness serving, then get out of the way.
 *
 * Phase 3's loader — environment checks, install, start, then the frame takes
 * over. The management plane replaces this in Phase 4; this page exists to be
 * honest about what the shell is doing while it does it.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { AlertTriangle, Download, Loader2, Play, RefreshCw } from 'lucide-react'

import { HarnessFrame } from '@/console/HarnessFrame'
import { describe } from '@/lib/errors'
import * as ipc from '@/lib/ipc'
import { Badge } from '@/ui/Badge'
import { Button } from '@/ui/Button'

type Phase =
  | { step: 'checking' }
  | { step: 'needs-install'; environment: ipc.Environment }
  | { step: 'installing' }
  | { step: 'starting' }
  | { step: 'ready'; origin: string }
  | { step: 'failed'; reason: string }

export function Bootstrap() {
  const [phase, setPhase] = useState<Phase>({ step: 'checking' })
  const [log, setLog] = useState<ipc.LogLine[]>([])
  const startedRef = useRef(false)

  const refresh = useCallback(async () => {
    setPhase({ step: 'checking' })
    try {
      const environment = await ipc.environment()
      if (!environment.harnessInstalled) {
        setPhase({ step: 'needs-install', environment })
        return
      }
      setPhase({ step: 'starting' })
      const origin = await ipc.start()
      setPhase({ step: 'ready', origin })
    } catch (cause) {
      setPhase({ step: 'failed', reason: describe(cause) })
    }
  }, [])

  const install = useCallback(async () => {
    setPhase({ step: 'installing' })
    try {
      await ipc.install()
      await refresh()
    } catch (cause) {
      setPhase({ step: 'failed', reason: describe(cause) })
    }
  }, [refresh])

  useEffect(() => {
    void refresh()
  }, [refresh])

  // The supervisor owns the process; this window only watches.
  useEffect(() => {
    let unlisten: (() => void) | undefined
    void ipc.onHarnessEvent((event) => {
      if (event.kind === 'log') {
        setLog((previous) => [...previous.slice(-199), { stream: event.stream, line: event.line }])
        return
      }
      const { kind: _kind, ...status } = event
      switch (status.phase) {
        case 'ready':
          setPhase({ step: 'ready', origin: status.origin })
          break
        case 'restarting':
        case 'starting':
          setPhase((previous) =>
            previous.step === 'ready' ? previous : { step: 'starting' },
          )
          break
        case 'failed':
          setPhase({ step: 'failed', reason: status.reason })
          break
        default:
          break
      }
    }).then((stop) => {
      unlisten = stop
    })
    return () => unlisten?.()
  }, [])

  // One automatic start attempt per window; retries are user-driven.
  useEffect(() => {
    if (startedRef.current || phase.step !== 'starting') return
    startedRef.current = true
  }, [phase.step])

  const tail = log.slice(-14)

  return (
    <div className="flex h-full flex-col">
      <main className="relative min-h-0 flex-1">
        {phase.step === 'ready' ? (
          <HarnessFrame origin={phase.origin} hidden={false} />
        ) : (
          <div className="flex h-full flex-col items-center justify-center gap-4 px-8">
            {phase.step === 'checking' && (
              <Pending label="Checking this machine for a usable runtime…" />
            )}
            {phase.step === 'starting' && (
              <Pending label="Starting the DeepSeek Harness service…" />
            )}
            {phase.step === 'installing' && (
              <Pending label={`Installing ${'@deepseek-ai/dsh'} — npm output streams below.`} />
            )}
            {phase.step === 'needs-install' && (
              <section className="flex max-w-md flex-col items-center gap-3 text-center">
                <Download size={20} className="text-brand" aria-hidden="true" />
                <h2 className="text-[13.5px] font-semibold">
                  The Harness runtime is not installed yet
                </h2>
                <p className="text-[12px] text-muted">
                  HarnessLite installs the official package (pinned at{' '}
                  <span className="font-mono">{phase.environment.expectedHarnessVersion}</span>)
                  into its own data directory. This needs the network once.
                </p>
                <div className="flex items-center gap-2">
                  <Button onClick={() => void install()}>
                    <Download size={13} aria-hidden="true" /> Install now
                  </Button>
                  <Button variant="ghost" onClick={() => void refresh()}>
                    <RefreshCw size={13} aria-hidden="true" /> Re-check
                  </Button>
                </div>
                {phase.environment.node === null && (
                  <Badge tone="warn">no usable Node runtime found on this machine</Badge>
                )}
              </section>
            )}
            {phase.step === 'failed' && (
              <section className="flex max-w-lg flex-col items-center gap-3 text-center">
                <AlertTriangle size={20} className="text-warn" aria-hidden="true" />
                <h2 className="text-[13.5px] font-semibold">The harness did not start</h2>
                <p className="selectable max-h-40 overflow-auto whitespace-pre-wrap rounded-panel border border-line bg-surface p-3 text-left font-mono text-[11px] text-muted">
                  {phase.reason}
                </p>
                <div className="flex items-center gap-2">
                  <Button onClick={() => void refresh()}>
                    <Play size={13} aria-hidden="true" /> Try again
                  </Button>
                  <Button variant="ghost" onClick={() => void refresh()}>
                    <RefreshCw size={13} aria-hidden="true" /> Re-check environment
                  </Button>
                </div>
              </section>
            )}

            {tail.length > 0 && (
              <div className="absolute inset-x-4 bottom-3 max-h-40 overflow-auto rounded-panel border border-line bg-canvas-deep p-2 font-mono text-[10.5px] leading-relaxed text-faint">
                {tail.map((entry, index) => (
                  <div key={index} className={entry.stream === 'stderr' ? 'text-danger/80' : ''}>
                    {entry.line}
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </main>
    </div>
  )
}

function Pending({ label }: { label: string }) {
  return (
    <div className="flex items-center gap-2 text-[12.5px] text-muted">
      <Loader2 size={14} className="animate-spin text-brand" aria-hidden="true" />
      {label}
    </div>
  )
}
