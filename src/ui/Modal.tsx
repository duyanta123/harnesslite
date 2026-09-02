import { useEffect, useRef, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react'
import { X, type LucideIcon } from 'lucide-react'

import { holdFocus, pressedBackdrop } from '@/lib/modal'

/**
 * The shell every closable modal wears.
 *
 * Four dialogs used to hand-roll the same chrome — a dimmed backdrop, a card on
 * the app's surface, an icon beside a title, a close in the corner — and had
 * begun to drift: four paddings, three close buttons, one focus trap. This is
 * that chrome, drawn once. What a dialog *says* stays with the caller; the
 * shell owns only the facts every modal shares: it covers the window, it traps
 * Tab, Escape answers it, a press on the backdrop answers it, and focus goes
 * back to whatever the user was on when it closes.
 *
 * `z` is a prop because the layering is not decorative: the recovery centre has
 * to stand above the question dialog, which stands above the palette. The
 * caller states where its modal sits; the shell does not guess.
 */
export function Modal({
  icon,
  iconClassName = 'bg-surface-2 text-brand',
  title,
  breakTitle = false,
  subtitle,
  subtitleHint,
  onClose,
  closeLabel,
  variant = 'dialog',
  backdropClose = true,
  closable = true,
  onEscape,
  width = 540,
  z = 40,
  children,
  footer,
  footerClassName,
}: {
  icon: LucideIcon
  /** Tone classes for the icon tile — a danger modal tints the tile, not the shell. */
  iconClassName?: string
  title: string
  /** Package names and paths have no spaces to wrap at: break anywhere instead. */
  breakTitle?: boolean
  subtitle?: ReactNode
  /** Full text for the subtitle's tooltip, when it had to be shortened to fit. */
  subtitleHint?: string
  onClose: () => void
  /** The close button's accessible name, in the caller's language. */
  closeLabel: string
  /**
   * `alertdialog` for an interruption that wants answering before anything
   * else happens; the plain dialog is the default.
   */
  variant?: 'dialog' | 'alertdialog'
  /**
   * Whether a press on the dimmed area closes. False for a modal whose dismiss
   * is a decision — a press that lands a pixel outside the card should not
   * answer a question the user has not read yet.
   */
  backdropClose?: boolean
  /** Whether the corner X is drawn. False when dismissal is one of the answers
   * the modal offers, rather than a way out beside them. */
  closable?: boolean
  /**
   * First claim on Escape, for a modal that holds a question of its own — a
   * form open over the list. Return true when the key was consumed; otherwise
   * Escape closes this modal.
   */
  onEscape?: (event: KeyboardEvent<HTMLDivElement>) => boolean
  /** Card width in px. */
  width?: number
  /** Where this modal sits in the window's stacking order. */
  z?: number
  children: ReactNode
  footer?: ReactNode
  footerClassName?: string
}) {
  const card = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const previous = document.activeElement
    card.current?.focus()
    return () => {
      if (previous instanceof HTMLElement) previous.focus()
    }
  }, [])

  // Escape says no, Tab stays in here: outside a modal the rest of the window
  // is still focusable, and a caret that walks out of a modal and into the pane
  // behind it is how a modal stops being one.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (onEscape?.(event) === true) return
    holdFocus(card.current, event, onClose)
  }

  const onBackdrop = (event: MouseEvent<HTMLDivElement>) => pressedBackdrop(event, onClose)

  const Icon = icon

  return (
    <div
      role="presentation"
      onMouseDown={backdropClose ? onBackdrop : undefined}
      onKeyDown={onKeyDown}
      style={{ zIndex: z }}
      className="fixed inset-0 grid animate-fade place-items-center bg-canvas-deep/70 px-8 backdrop-blur-[2px]"
    >
      <div
        ref={card}
        role={variant}
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        style={{ maxWidth: width }}
        className="lift-top flex max-h-[86vh] w-full animate-pop flex-col rounded-panel border border-line-strong bg-surface shadow-lift outline-none"
      >
        <header className="flex shrink-0 items-start gap-3 border-b border-line px-4 py-3.5">
          <span
            aria-hidden="true"
            className={`grid size-9 shrink-0 place-items-center rounded-[9px] ${iconClassName}`}
          >
            <Icon size={17} strokeWidth={2} />
          </span>
          <div className="min-w-0 flex-1 pt-0.5">
            <h2
              className={[
                'text-[13.5px] leading-snug font-semibold text-text',
                breakTitle ? 'selectable break-all' : 'truncate',
              ].join(' ')}
            >
              {title}
            </h2>
            {subtitle && (
              <div className="mt-0.5 text-[11.5px] text-faint" data-hint={subtitleHint}>
                {subtitle}
              </div>
            )}
          </div>
          {closable && (
            <button
              type="button"
              aria-label={closeLabel}
              data-hint={closeLabel}
              onClick={onClose}
              className="-mr-1 grid size-7 shrink-0 place-items-center rounded-control text-faint/80 transition-colors duration-100 hover:text-muted"
            >
              <X size={14} strokeWidth={2.2} aria-hidden="true" />
            </button>
          )}
        </header>

        <div className="flex min-h-0 flex-1 flex-col">{children}</div>

        {footer && (
          <div className={`shrink-0 border-t border-line px-4 py-3 ${footerClassName ?? ''}`}>
            {footer}
          </div>
        )}
      </div>
    </div>
  )
}
