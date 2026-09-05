import { useEffect, useRef, type ReactNode } from 'react'
import { X, type LucideIcon } from 'lucide-react'

import { t } from '@/lib/i18n'

/**
 * The full-window host every management surface wears.
 *
 * The conversation pane is this app's main view, so management replaces it
 * while it is open — the iframe stays mounted behind the cover — and closing
 * hands the window straight back to the session. Escape answers first and the
 * close button is there for the mouse.
 *
 * A management surface is a room of its own: two columns of settings, a
 * workbench of plugins, a terminal. None of them read well as a column pinned
 * to the window's edge, so the cover takes the whole window.
 */
export function Sheet({
  icon,
  title,
  onClose,
  children,
  footer,
}: {
  icon: LucideIcon
  title: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
}) {
  const panel = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const previous = document.activeElement
    panel.current?.focus()
    return () => {
      if (previous instanceof HTMLElement) previous.focus()
    }
  }, [])

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.stopPropagation()
        onClose()
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [onClose])

  const Icon = icon

  return (
    <div
      ref={panel}
      role="dialog"
      aria-modal="true"
      aria-label={title}
      tabIndex={-1}
      className="absolute inset-0 z-30 flex animate-fade flex-col bg-surface outline-none"
    >
      <header className="flex h-11 shrink-0 items-center gap-2.5 border-b border-line px-3">
        <span
          aria-hidden="true"
          className="grid size-7 shrink-0 place-items-center rounded-[8px] bg-surface-2 text-brand"
        >
          <Icon size={15} strokeWidth={2} />
        </span>
        <h2 className="min-w-0 flex-1 truncate text-[12.5px] font-semibold text-text">
          {title}
        </h2>
        <button
          type="button"
          aria-label={t('window.close')}
          onClick={onClose}
          className="grid size-7 shrink-0 place-items-center rounded-control text-faint/80 transition-colors duration-100 hover:text-muted"
        >
          <X size={14} strokeWidth={2.2} aria-hidden="true" />
        </button>
      </header>

      {/* A flex column, not a plain scroller: every pane root is a `flex-1`
          section that paints its own full-height background and scrolls
          inside itself, and flex-1 only stretches against a flex parent. */}
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">{children}</div>

      {footer && (
        <div className="shrink-0 border-t border-line px-3 py-2.5">{footer}</div>
      )}
    </div>
  )
}
