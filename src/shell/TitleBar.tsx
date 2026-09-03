import { getCurrentWindow } from '@tauri-apps/api/window'
import { Folder, KeyRound, Search, Minus, Square, X } from 'lucide-react'
import { useEffect, useState } from 'react'

import { t } from '@/lib/i18n'
import { labelOf } from '@/lib/status'
import type { Status } from '@/lib/ipc'
import { useProjects } from '@/state/projects'
import { useProfiles } from '@/state/profiles'

/**
 * The window's chrome: brand, the two identity chips, the palette entry and
 * the window buttons, all on one drag strip.
 *
 * The chips are the two facts every other surface is relative to — which
 * project's folder the harness works in, and which profile's composition it
 * runs. Both open their manager rather than a menu of their own: the title bar
 * names the state, the managers change it.
 */
export function TitleBar({
  status,
  onOpenProjects,
  onOpenProfiles,
  onOpenPalette,
}: {
  status: Status
  onOpenProjects: () => void
  onOpenProfiles: () => void
  onOpenPalette: () => void
}) {
  const roster = useProjects((state) => state.roster)
  const profiles = useProfiles((state) => state.roster)
  const [phase, setPhase] = useState<string>('')

  // Window controls live behind the desktop bridge; in the browser self-check
  // they simply do nothing rather than breaking the render.
  const [desktop, setDesktop] = useState(false)
  useEffect(() => {
    try {
      void getCurrentWindow().isVisible().then(() => setDesktop(true))
    } catch {
      setDesktop(false)
    }
  }, [])

  useEffect(() => {
    setPhase(labelOf(status))
  }, [status])

  const project =
    roster?.projects.find((project) => project.id === roster?.selected) ?? null
  const profile = profiles?.profiles.find((entry) => entry.name === profiles.selected)?.name ?? null

  return (
    <header
      data-tauri-drag-region
      className="chrome flex h-10 shrink-0 items-center gap-1.5 border-b border-line pl-3"
    >
      <div className="flex min-w-0 items-center gap-1.5" data-tauri-drag-region>
        <span
          aria-hidden="true"
          className="grid size-5 shrink-0 place-items-center rounded-[6px] bg-brand text-[10px] font-bold text-on-brand"
        >
          HL
        </span>
        <span className="truncate text-[12.5px] font-semibold text-text">
          HarnessLite
        </span>
        <span className="ml-1 shrink-0 rounded-full bg-surface-2 px-1.5 py-0.5 text-[10.5px] leading-none text-muted">
          {phase}
        </span>
      </div>

      <div className="ml-3 flex min-w-0 items-center gap-1">
        <button
          type="button"
          onClick={onOpenProjects}
          title={t('project.manage')}
          className="flex h-[22px] min-w-0 shrink items-center gap-1 rounded-control border border-line bg-surface-2 px-1.5 text-[11px] text-muted transition-colors duration-100 hover:text-text"
        >
          <Folder size={11} aria-hidden="true" />
          <span className="max-w-[140px] truncate">
            {project?.name ?? t('project.manage')}
          </span>
        </button>
        <button
          type="button"
          onClick={onOpenProfiles}
          title={t('profile.title')}
          className="flex h-[22px] min-w-0 shrink items-center gap-1 rounded-control border border-line bg-surface-2 px-1.5 text-[11px] text-muted transition-colors duration-100 hover:text-text"
        >
          <KeyRound size={11} aria-hidden="true" />
          <span className="max-w-[140px] truncate">{profile ?? t('profile.title')}</span>
        </button>
      </div>

      <button
        type="button"
        onClick={onOpenPalette}
        title="Ctrl+K"
        className="ml-auto mr-1 grid size-7 shrink-0 place-items-center rounded-control text-faint/80 transition-colors duration-100 hover:text-muted"
      >
        <Search size={13} strokeWidth={2} aria-hidden="true" />
      </button>

      {desktop && (
        <div className="flex h-full shrink-0 items-stretch">
          <WindowButton label={t('window.minimize')} onClick={() => void getCurrentWindow().minimize()}>
            <Minus size={13} aria-hidden="true" />
          </WindowButton>
          <WindowButton
            label={t('window.maximize')}
            onClick={() => void getCurrentWindow().toggleMaximize()}
          >
            <Square size={11} aria-hidden="true" />
          </WindowButton>
          <WindowButton label={t('window.close')} danger onClick={() => void getCurrentWindow().close()}>
            <X size={14} aria-hidden="true" />
          </WindowButton>
        </div>
      )}
    </header>
  )
}

function WindowButton({
  label,
  danger = false,
  onClick,
  children,
}: {
  label: string
  danger?: boolean
  onClick: () => void
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`grid w-11 place-items-center text-muted transition-colors duration-100 ${
        danger ? 'hover:bg-danger hover:text-on-danger' : 'hover:bg-surface-2 hover:text-text'
      }`}
    >
      {children}
    </button>
  )
}
