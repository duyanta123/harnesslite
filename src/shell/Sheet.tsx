import { useEffect, useRef, type ReactNode } from 'react'
import { X, type LucideIcon } from 'lucide-react'

import { t } from '@/lib/i18n'

/**
 * The slide-over host every management surface wears.
 *
 * The conversation pane is this app's main view, so management never replaces
 * it — a sheet covers it from the right while the iframe stays mounted behind,
 * and closing the sheet hands the window straight back to the session. Escape
 * answers first, a press on the backdrop answers, and the close button is
 * there for the mouse.
 *
 * `wide` is for surfaces that read like a workbench (the terminal, the plugin
 * market); everything else is a column, because a sheet is a visit, not a room.
 */
export function Sheet({
  icon,
  title,
  onClose,
  wide = false,
  children,
  footer,
}: {
  icon: LucideIcon
  title: string
  onClose: () => void
  /** Workbench-width surfaces read better wide than tall. */
  wide?: boolean
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
      role="presentation"
      className="fixed inset-0 z-30 animate-fade"
      onMouseDown={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div
        ref={panel}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        style={{ width: wide ? 720 : 440 }}
        className="lift-top absolute inset-y-0 right-0 flex animate-slide-forward flex-col border-l border-line-strong bg-surface shadow-lift outline-none"
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

        <div className="min-h-0 flex-1 overflow-y-auto">{children}</div>

        {footer && (
          <div className="shrink-0 border-t border-line px-3 py-2.5">{footer}</div>
        )}
      </div>
    </div>
  )
}
