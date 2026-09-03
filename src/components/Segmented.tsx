import { useLayoutEffect, useRef, useState, type KeyboardEvent } from 'react'

export interface SegmentOption<T extends string | number> {
  value: T
  label: string
}

/**
 * A recessed track with one raised thumb.
 *
 * Segments rather than underlined tabs, because these strips sit inside pane
 * headers where an underline would be a second horizontal rule next to the one
 * already there. What makes it a control rather than a row of buttons is the
 * thumb: one raised surface that slides from the old selection to the new one,
 * so a change reads as the selection *moving*, and the eye follows it instead
 * of re-scanning the strip to find what changed.
 *
 * The thumb is measured against the live DOM rather than computed from the
 * option list — labels are not equal width, and a thumb that assumed they were
 * would be visibly wrong on the first CNY/USD pair.
 *
 * Keyboard: the arrows move the selection directly (radio-group semantics),
 * because Tab's job here is to leave the strip, not to walk it.
 */
export function Segmented<T extends string | number>({
  options,
  value,
  onChange,
  label,
}: {
  options: SegmentOption<T>[]
  value: T
  onChange: (next: T) => void
  label: string
}) {
  const track = useRef<HTMLDivElement>(null)
  const [thumb, setThumb] = useState<{ left: number; width: number } | null>(null)

  // Measured in the same frame the value moves, and re-measured whenever the
  // track itself changes size — a pane that widens must not leave its thumb
  // behind on the old geometry.
  useLayoutEffect(() => {
    const container = track.current
    if (!container) return

    const measure = () => {
      const active = container.querySelector<HTMLButtonElement>('[aria-checked="true"]')
      if (!active) return
      setThumb((current) =>
        current &&
        current.left === active.offsetLeft &&
        current.width === active.offsetWidth
          ? current
          : { left: active.offsetLeft, width: active.offsetWidth },
      )
    }

    measure()
    const watch = new ResizeObserver(measure)
    watch.observe(container)
    return () => watch.disconnect()
  }, [value, options])

  const step = (direction: 1 | -1) => {
    const at = options.findIndex((option) => option.value === value)
    if (at === -1) return
    const next = options[(at + direction + options.length) % options.length]
    if (next) onChange(next.value)
  }

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown') {
      event.preventDefault()
      step(1)
    }
    if (event.key === 'ArrowLeft' || event.key === 'ArrowUp') {
      event.preventDefault()
      step(-1)
    }
  }

  return (
    <div
      ref={track}
      role="radiogroup"
      aria-label={label}
      onKeyDown={onKeyDown}
      className="relative flex items-center gap-0.5 rounded-control bg-canvas-deep p-0.5 hairline"
    >
      {thumb && (
        <span
          aria-hidden="true"
          className="raised absolute bottom-0.5 top-0.5 rounded-[4px] bg-surface-2 transition-[left] duration-150 ease-[var(--ease-out-soft)]"
          style={{ left: thumb.left, width: thumb.width }}
        />
      )}

      {options.map((option) => {
        const active = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={active}
            tabIndex={active ? 0 : -1}
            onClick={() => onChange(option.value)}
            className={[
              'relative z-10 h-[22px] rounded-[4px] px-2.5 text-[11.5px] whitespace-nowrap transition-colors duration-100',
              active ? 'cursor-default text-text' : 'text-faint hover:text-muted',
            ].join(' ')}
          >
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
