import type { ReactNode } from 'react'

export type BadgeTone = 'ok' | 'warn' | 'danger' | 'neutral' | 'brand'

const TONE: Record<BadgeTone, string> = {
  ok: 'bg-ok/12 text-ok',
  warn: 'bg-warn/12 text-warn',
  danger: 'bg-danger/12 text-danger',
  neutral: 'bg-surface-2 text-muted',
  brand: 'bg-brand/12 text-brand',
}

/**
 * One pill for the small facts lists carry.
 *
 * Five panes used to draw their own — same shape, five paddings, four type
 * sizes — and the drift showed exactly where a list met another list. A badge
 * states a fact about its row ("active", "builtin", "3.2 kB"); it is never a
 * button, and its tones carry status meaning, which is why they stay the
 * palette's status colours rather than the accent gradient.
 */
export function Badge({
  tone = 'neutral',
  children,
}: {
  tone?: BadgeTone
  children: ReactNode
}) {
  return (
    <span
      className={`inline-flex shrink-0 items-center rounded-full px-1.5 py-0.5 text-[10.5px] leading-none font-medium ${TONE[tone]}`}
    >
      {children}
    </span>
  )
}
