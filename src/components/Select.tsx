import { ChevronDown } from 'lucide-react'
import type { ComponentPropsWithRef } from 'react'

/**
 * The native select, dressed.
 *
 * The popup stays the platform's — a desktop tool does not gain from a web
 * listbox re-drawn over the real one, and the platform popup is where keyboard
 * and screen-reader behaviour come for free. What this dresses is the closed
 * control: one height, one border, one hand-drawn chevron, so four selects
 * across the settings pane read as one control rather than as four frames the
 * browser happened to paint.
 *
 * The name is a required prop rather than a passed-through attribute, because
 * a control with no label is a bug this component is in the position to make
 * unrepresentable.
 */
export function Select({
  label,
  compact = false,
  className,
  wrapperClassName,
  children,
  ...rest
}: Omit<ComponentPropsWithRef<'select'>, 'aria-label'> & {
  /** Spoken as the control's name; also what the wrapped `<select>` is labelled by. */
  label: string
  /** The 28px size for dense toolbar rows; the default 30px is the app's control height. */
  compact?: boolean
  /** Sizing for the wrapper, which is what flexes inside a form row. */
  wrapperClassName?: string
}) {
  return (
    <span className={`relative inline-flex ${wrapperClassName ?? ''}`}>
      <select
        aria-label={label}
        className={[
          'cursor-pointer appearance-none rounded-control border border-line-strong bg-surface-2 text-text outline-none focus:border-brand disabled:opacity-40',
          compact ? 'h-[28px] pr-7 pl-2 text-[11px]' : 'h-[30px] pr-7 pl-2.5 text-[11.5px]',
          className,
        ]
          .filter(Boolean)
          .join(' ')}
        {...rest}
      >
        {children}
      </select>
      <ChevronDown
        size={12}
        strokeWidth={2.2}
        aria-hidden="true"
        className="pointer-events-none absolute top-1/2 right-2 -translate-y-1/2 text-faint"
      />
    </span>
  )
}
