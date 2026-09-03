import { useState } from 'react'
import type { LucideIcon } from 'lucide-react'
import { ChevronRight, Loader2 } from 'lucide-react'

import { Button } from '@/components/Button'

export type CheckState = 'ok' | 'missing' | 'neutral'

/** The one thing that would fix this row, offered on the row itself. */
export interface CheckAction {
  label: string
  icon: LucideIcon
  busy?: boolean
  run: () => void
}

export interface CheckItem {
  key: string
  label: string
  value: string
  /** Full text when `value` had to be shortened to fit. */
  title?: string
  state: CheckState
  action?: CheckAction
}

/*
 * The state is one ring that changes what it carries, not three glyphs that
 * swap places: `ok` keeps the ring and draws a check inside it, `missing`
 * keeps the ring and draws a cross, `neutral` keeps the ring and marks a dash.
 * Because the ring never unmounts, the eye tracks a single object down the
 * list instead of re-finding a new icon on every row.
 */
const RING: Record<CheckState, string> = {
  ok: 'var(--color-ok)',
  missing: 'var(--color-danger)',
  neutral: 'var(--color-faint)',
}

const MARK: Record<CheckState, { d: string; delay: number }[]> = {
  ok: [{ d: 'M 8.2 12.3 L 10.9 15 L 15.9 9.5', delay: 0 }],
  missing: [
    { d: 'M 9.2 9.2 L 14.8 14.8', delay: 0 },
    { d: 'M 14.8 9.2 L 9.2 14.8', delay: 0.06 },
  ],
  neutral: [{ d: 'M 9.4 12 L 14.6 12', delay: 0 }],
}

/** The ring and the mark it carries, drawn rather than borrowed from a font. */
function StatusRing({ state }: { state: CheckState }) {
  const ring = RING[state]

  return (
    <svg
      viewBox="0 0 24 24"
      width={16}
      height={16}
      aria-hidden="true"
      className="shrink-0 overflow-visible"
    >
      <circle
        cx={12}
        cy={12}
        r={8}
        fill="none"
        stroke={ring}
        strokeWidth={2}
        pathLength={1}
        strokeDasharray={state === 'neutral' ? '0.2 0.06' : '1 0'}
        opacity={state === 'neutral' ? 0.7 : 1}
      />
      {MARK[state].map((mark) => (
        <path
          key={mark.d}
          d={mark.d}
          fill="none"
          stroke={ring}
          strokeWidth={2.1}
          strokeLinecap="round"
          strokeLinejoin="round"
          pathLength={1}
          strokeDasharray={1}
          className="animate-draw"
          style={{ animationDelay: `${mark.delay}s` }}
        />
      ))}
    </svg>
  )
}

/**
 * The pre-flight checks, as one bordered list.
 *
 * The state is a ring badge and the row is read by scanning down the left
 * edge, which is why the ring is the only decoration a row carries. A row
 * whose full evidence does not fit the value column — an absolute path, a
 * multi-sentence verdict — grows a chevron, and the evidence unfolds under the
 * row rather than living in a hover tooltip nobody on a touch pad can aim at.
 *
 * A row that reports something missing still carries the fix next to it —
 * being told what is wrong and then left to solve it elsewhere is the failure
 * this avoids.
 */
export function CheckList({ items }: { items: CheckItem[] }) {
  return (
    <ul className="divide-y divide-line overflow-hidden rounded-panel border border-line bg-canvas-deep/50">
      {items.map((item) => (
        <CheckRow key={item.key} item={item} />
      ))}
    </ul>
  )
}

function CheckRow({ item }: { item: CheckItem }) {
  // Only rows with hidden evidence may open. The flag lives per row, because
  // one shared index would mean two lists fighting over a number.
  const [open, setOpen] = useState(false)
  const expandable = Boolean(item.title && item.title !== item.value)
  const ActionIcon = item.action?.icon

  return (
    <li>
      <div className="flex h-[34px] items-center gap-2 px-2.5">
        <StatusRing state={item.state} />

        <span className="shrink-0 text-[12.5px] text-text">{item.label}</span>

        <span
          className="ml-auto truncate text-right font-mono text-[11.5px] text-muted"
          data-hint={expandable ? undefined : item.value}
        >
          {item.value}
        </span>

        {item.action && ActionIcon && (
          <Button
            variant="secondary"
            size="sm"
            onClick={item.action.run}
            disabled={item.action.busy}
          >
            {item.action.busy ? (
              <Loader2 size={11} className="animate-spin" aria-hidden="true" />
            ) : (
              <ActionIcon size={11} strokeWidth={2.4} aria-hidden="true" />
            )}
            {item.action.label}
          </Button>
        )}

        {expandable && (
          <button
            type="button"
            aria-expanded={open}
            aria-controls={`check-${item.key}-detail`}
            aria-label={item.title}
            onClick={() => setOpen(!open)}
            className="-mr-1 grid size-[22px] shrink-0 place-items-center rounded-[4px] text-faint transition-colors duration-100 hover:bg-surface-2 hover:text-text"
          >
            <ChevronRight
              size={12}
              strokeWidth={2.6}
              aria-hidden="true"
              className={`transition-transform duration-150 ease-[var(--ease-out-soft)] ${open ? 'rotate-90' : ''}`}
            />
          </button>
        )}
      </div>

      {/* Unmounted rather than hidden: an open detail is a reading state, and a
          closed one has nothing to keep. */}
      {open && expandable && (
        <p
          id={`check-${item.key}-detail`}
          className="selectable break-all px-2.5 pb-2.5 pl-[38px] font-mono text-[11px] leading-relaxed text-faint"
        >
          {item.title}
        </p>
      )}
    </li>
  )
}
