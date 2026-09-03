import { useEffect, useMemo, useRef, useState } from 'react'
import { Search } from 'lucide-react'

import { fuzzy, segments } from '@/lib/fuzzy'
import { t } from '@/lib/i18n'
import { usePalette } from '@/state/palette'

/**
 * One command anyone can type.
 *
 * The roster is rebuilt on every open from live state, so nothing here can go
 * stale; recents are ids resolved against that roster, and the dead ones drop
 * off quietly.
 */
export interface OmniCommand {
  id: string
  label: string
  hint?: string
  run: () => void
}

/** Highlight what the query matched, the way the matcher scored it. */
function Highlighted({ text, at }: { text: string; at: number[] }) {
  return (
    <>
      {segments(text, at).map((part, index) =>
        part.hit ? (
          <mark key={index}>{part.text}</mark>
        ) : (
          <span key={index}>{part.text}</span>
        ),
      )}
    </>
  )
}

export function Omni({
  commands,
  onClose,
}: {
  /** Resolved live by the caller on every render the palette is open. */
  commands: OmniCommand[]
  onClose: () => void
}) {
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const input = useRef<HTMLInputElement>(null)
  const recents = usePalette((state) => state.recents)
  const recordRecent = usePalette((state) => state.recordRecent)

  const matches = useMemo(() => {
    const scored = commands
      .map((command) => {
        if (query === '') {
          const recentAt = recents.indexOf(command.id)
          return { command, at: [] as number[], rank: recentAt === -1 ? Number.MAX_SAFE_INTEGER : recentAt }
        }
        const match = fuzzy(query, command.label)
        return match ? { command, at: match.at, rank: match.score } : null
      })
      .filter((entry): entry is { command: OmniCommand; at: number[]; rank: number } => entry !== null)

    if (query === '') {
      return scored.sort((left, right) => left.rank - right.rank).map(({ command, at }) => ({ command, at }))
    }
    return scored.sort((left, right) => left.rank - right.rank).map(({ command, at }) => ({ command, at }))
  }, [commands, query, recents])

  useEffect(() => {
    setSelected(0)
  }, [query])

  useEffect(() => {
    input.current?.focus()
  }, [])

  const run = (command: OmniCommand | undefined) => {
    if (!command) return
    recordRecent(command.id)
    onClose()
    command.run()
  }

  return (
    <div
      role="presentation"
      className="fixed inset-0 z-50 grid animate-fade place-items-start bg-canvas-deep/70 px-8 pt-[18vh] backdrop-blur-[2px]"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.stopPropagation()
          onClose()
          return
        }
        if (event.key === 'ArrowDown') {
          event.preventDefault()
          setSelected((at) => Math.min(at + 1, matches.length - 1))
        }
        if (event.key === 'ArrowUp') {
          event.preventDefault()
          setSelected((at) => Math.max(at - 1, 0))
        }
        if (event.key === 'Enter') {
          event.preventDefault()
          run(matches[selected]?.command)
        }
      }}
    >
      <div className="lift-top flex max-h-[52vh] w-full max-w-[520px] animate-pop flex-col overflow-hidden rounded-panel border border-line-strong bg-surface shadow-lift">
        <div className="flex h-11 shrink-0 items-center gap-2.5 border-b border-line px-3.5">
          <Search size={14} className="shrink-0 text-faint" aria-hidden="true" />
          <input
            ref={input}
            type="text"
            role="combobox"
            aria-expanded
            aria-label={t('palette.title')}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t('palette.search')}
            className="min-w-0 flex-1 bg-transparent text-[13px] text-text outline-none placeholder:text-faint"
          />
        </div>

        <div role="listbox" aria-label={t('palette.title')} className="min-h-0 flex-1 overflow-y-auto p-1.5">
          {matches.length === 0 && (
            <p className="px-2.5 py-6 text-center text-[12px] text-faint">{t('palette.empty')}</p>
          )}
          {matches.map(({ command, at }, index) => (
            <button
              key={command.id}
              type="button"
              role="option"
              aria-selected={index === selected}
              onMouseEnter={() => setSelected(index)}
              onClick={() => run(command)}
              className={`flex w-full items-center gap-2.5 rounded-control px-2.5 py-2 text-left text-[12.5px] ${
                index === selected ? 'bg-surface-2 text-text' : 'text-muted'
              }`}
            >
              <span className="min-w-0 flex-1 truncate">
                <Highlighted text={command.label} at={at} />
              </span>
              {command.hint && <span className="shrink-0 text-[10.5px] text-faint">{command.hint}</span>}
            </button>
          ))}
        </div>
      </div>
    </div>
  )
}
