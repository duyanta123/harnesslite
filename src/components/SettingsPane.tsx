import { useEffect, useState } from 'react'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import type { ComponentType, KeyboardEvent, ReactNode } from 'react'
import {
  BellRing,
  CircleCheck,
  CircleX,
  Folder,
  FolderPlus,
  Pencil,
  Trash2,
  Keyboard,
  Network,
  Power,
  PanelsTopLeft,
  ScrollText,
  Server,
  SlidersHorizontal,
  SunMoon,
  TriangleAlert,
  X,
} from 'lucide-react'

import { Button } from '@/components/Button'
import { PaneHeader } from '@/components/PaneHeader'
import { Select } from '@/components/Select'
import { Switch } from '@/components/Switch'
import { ThemeSwitch } from '@/components/ThemeSwitch'
import { t, type MessageKey } from '@/lib/i18n'
import { readCombination, spellCombination } from '@/lib/keys'
import { isMac } from '@/lib/platform'
import { useStartup } from '@/state/startup'
import { ask } from '@/state/dialog'
import { useProfiles } from '@/state/profiles'
import { addProjectWorkspace, switchProject, useProjects } from '@/state/projects'
import { usePresentation } from '@/state/presentation'

/**
 * Settings that outlive the window.
 *
 * Everything else this app can be told is a property of a profile, a session or
 * a plugin, and lives next to the thing it changes. What is left is the handful
 * of facts about the machine rather than the work: whether the app is here at
 * login, which key reaches it from anywhere, and which palette it paints. So
 * this pane is deliberately short, and it stays short — a settings screen that
 * collects every switch in the app is the place options go to be lost.
 *
 * The first two are off until asked for. A login item nobody agreed to is the
 * reason people distrust installers, and a global key is taken away from every
 * other program on the machine, including the editor that had it first.
 */
export function SettingsPane() {
  const state = useStartup((store) => store.state)
  const busy = useStartup((store) => store.busy)
  const error = useStartup((store) => store.error)
  const refresh = useStartup((store) => store.refresh)
  const setAutostart = useStartup((store) => store.setAutostart)
  const setNotification = useStartup((store) => store.setNotification)
  const testNotification = useStartup((store) => store.testNotification)
  const setLogLevel = useStartup((store) => store.setLogLevel)
  const setHarnessPort = useStartup((store) => store.setHarnessPort)
  const retry = useStartup((store) => store.retry)
  const presentation = usePresentation((store) => store.mode)
  const choosePresentation = usePresentation((store) => store.choose)

  // Asked again on every visit rather than once at launch: both of these can be
  // taken away from outside the app — the login item from Task Manager, the key
  // by whatever started next — and this pane is where somebody comes to look.
  useEffect(() => {
    void refresh()
  }, [refresh])

  // Held here rather than inside the recorder because the way out of it belongs
  // under the label, where there is room for a sentence — a button in the middle
  // of listening for a keystroke is the last place to explain itself.
  const [recording, setRecording] = useState(false)
  const [section, setSection] = useState<SectionId>('projects')

  const ready = state !== null
  const occupied = Boolean(state?.shortcut) && state?.held === false
  const active = SECTIONS.find((entry) => entry.id === section) ?? SECTIONS[0]
  if (!active) return null

  return (
    <section className="flex min-h-0 flex-1 animate-rise flex-col">
      <PaneHeader title={t('settings.title')} subtitle={t('settings.subtitle')} />

      <div className="flex min-h-0 flex-1">
        {/* The nav the reference design centres on: a quiet column of sections
            with the live one marked the same way the workbench rail marks its
            pane — one bar, on the edge, in the accent. */}
        <nav
          aria-label={t('settings.title')}
          className="chrome flex w-[184px] shrink-0 flex-col gap-0.5 border-r border-line p-2"
        >
          {SECTIONS.map((entry) => {
            const chosen = entry.id === section
            const Icon = entry.icon
            return (
              <button
                key={entry.id}
                type="button"
                aria-current={chosen ? 'true' : undefined}
                onClick={() => setSection(entry.id)}
                className={[
                  'relative flex h-9 shrink-0 cursor-pointer items-center gap-2.5 rounded-control px-2.5 text-left transition-colors duration-100',
                  chosen ? 'bg-surface-2 text-text' : 'text-muted hover:bg-surface-2/55 hover:text-text',
                ].join(' ')}
              >
                {chosen && (
                  <span
                    aria-hidden="true"
                    className="absolute top-1/2 left-0 h-[16px] w-[2.5px] -translate-y-1/2 rounded-full bg-brand"
                  />
                )}
                <Icon
                  size={15}
                  strokeWidth={chosen ? 2.2 : 1.9}
                  className={`shrink-0 ${chosen ? 'text-brand' : 'text-faint'}`}
                  aria-hidden="true"
                />
                <span className="min-w-0 truncate text-[12.5px] font-medium">
                  {t(entry.label)}
                </span>
              </button>
            )
          })}
        </nav>

        <div className="min-h-0 flex-1 overflow-y-auto bg-canvas px-6 py-6">
          {/* Keyed so a section switch replays the settle animation, the same
              contract the workbench uses between panes. */}
          <div key={section} className="mx-auto flex max-w-[560px] animate-rise flex-col gap-4">
            <header className="flex flex-col gap-1">
              <h2 className="text-[14.5px] font-semibold text-text">{t(active.label)}</h2>
              <p className="text-[12px] leading-relaxed text-faint">{t(active.hint)}</p>
            </header>

            {section === 'projects' && <ProjectsSection />}

            {section === 'behavior' && (
              <div className="divide-y divide-line overflow-hidden rounded-panel border border-line bg-canvas-deep/50">
                <Row icon={Power} label={t('settings.autostart')} hint={t('settings.autostartHint')}>
                  <Switch
                    on={state?.autostart ?? false}
                    busy={busy}
                    disabled={!ready}
                    label={t('settings.autostart')}
                    onChange={(on) => void setAutostart(on)}
                  />
                </Row>

                <Row
                  icon={Keyboard}
                  label={t('settings.shortcut')}
                  hint={t('settings.shortcutHint')}
                  note={
                    recording ? (
                      <span className="text-brand">{t('settings.recordingHint')}</span>
                    ) : (
                      occupied && (
                        <span className="flex items-center gap-2 text-warn">
                          <TriangleAlert size={12} strokeWidth={2.2} aria-hidden="true" />
                          {t('settings.taken')}
                          <button
                            type="button"
                            onClick={() => void retry()}
                            disabled={busy}
                            className="shrink-0 font-medium underline decoration-warn/40 underline-offset-2 transition-colors duration-100 enabled:hover:decoration-warn"
                          >
                            {t('settings.retake')}
                          </button>
                        </span>
                      )
                    )
                  }
                >
                  <Recorder recording={recording} onRecording={setRecording} />
                </Row>

                <Row
                  icon={SunMoon}
                  label={t('settings.appearance')}
                  hint={t('settings.appearanceHint')}
                >
                  <ThemeSwitch />
                </Row>

                <Row
                  icon={PanelsTopLeft}
                  label={t('settings.presentation')}
                  hint={t('settings.presentationHint')}
                >
                  <Select
                    value={presentation}
                    label={t('settings.presentation')}
                    onChange={(event) =>
                      choosePresentation(
                        event.target.value as 'compatibility' | 'extended' | 'advanced',
                      )
                    }
                  >
                    <option value="compatibility">{t('settings.presentation.compatibility')}</option>
                    <option value="extended">{t('settings.presentation.extended')}</option>
                    <option value="advanced">{t('settings.presentation.advanced')}</option>
                  </Select>
                </Row>
              </div>
            )}

            {section === 'service' && (
              <div className="divide-y divide-line overflow-hidden rounded-panel border border-line bg-canvas-deep/50">
                <Row
                  icon={ScrollText}
                  label={t('settings.logLevel')}
                  hint={t('settings.logLevelHint')}
                >
                  <Select
                    value={state?.logLevel ?? 'info'}
                    label={t('settings.logLevel')}
                    disabled={!ready || busy}
                    onChange={(event) =>
                      void setLogLevel(event.target.value as 'debug' | 'info' | 'warn' | 'error')
                    }
                  >
                    <option value="debug">{t('settings.logLevel.debug')}</option>
                    <option value="info">{t('settings.logLevel.info')}</option>
                    <option value="warn">{t('settings.logLevel.warn')}</option>
                    <option value="error">{t('settings.logLevel.error')}</option>
                  </Select>
                </Row>

                <Row
                  icon={Network}
                  label={t('settings.harnessPort')}
                  hint={t('settings.harnessPortHint')}
                >
                  <PortField
                    key={state?.harnessPort ?? 'automatic'}
                    value={state?.harnessPort ?? null}
                    disabled={!ready || busy}
                    onSave={(port) => void setHarnessPort(port)}
                  />
                </Row>
              </div>
            )}

            {section === 'notifications' && (
              <div className="divide-y divide-line overflow-hidden rounded-panel border border-line bg-canvas-deep/50">
                <Row
                  icon={BellRing}
                  label={t('settings.turnCompleted')}
                  hint={t('settings.turnCompletedHint')}
                >
                  <Switch
                    on={state?.notifications.turnCompleted ?? true}
                    busy={busy}
                    disabled={!ready}
                    label={t('settings.turnCompleted')}
                    onChange={(on) => void setNotification('turn-completed', on)}
                  />
                </Row>

                <Row
                  icon={CircleX}
                  label={t('settings.turnFailed')}
                  hint={t('settings.turnFailedHint')}
                >
                  <Switch
                    on={state?.notifications.turnFailed ?? true}
                    busy={busy}
                    disabled={!ready}
                    label={t('settings.turnFailed')}
                    onChange={(on) => void setNotification('turn-failed', on)}
                  />
                </Row>

                <Row
                  icon={CircleCheck}
                  label={t('settings.jobCompleted')}
                  hint={t('settings.jobCompletedHint')}
                >
                  <Switch
                    on={state?.notifications.jobCompleted ?? true}
                    busy={busy}
                    disabled={!ready}
                    label={t('settings.jobCompleted')}
                    onChange={(on) => void setNotification('job-completed', on)}
                  />
                </Row>

                <Row
                  icon={TriangleAlert}
                  label={t('settings.jobFailed')}
                  hint={t('settings.jobFailedHint')}
                >
                  <Switch
                    on={state?.notifications.jobFailed ?? true}
                    busy={busy}
                    disabled={!ready}
                    label={t('settings.jobFailed')}
                    onChange={(on) => void setNotification('job-failed', on)}
                  />
                </Row>

                <Row
                  icon={BellRing}
                  label={t('settings.notificationTest')}
                  hint={t('settings.notificationTestHint')}
                >
                  <Button
                    variant="secondary"
                    className="h-[30px] px-2.5 text-[11.5px]"
                    disabled={!ready || busy}
                    onClick={() => void testNotification()}
                  >
                    {t('settings.notificationTestAction')}
                  </Button>
                </Row>
              </div>
            )}

            {error && (
              <p className="selectable rounded-control border border-danger/30 bg-danger/10 px-3 py-2 text-[12px] leading-relaxed text-danger">
                {error}
              </p>
            )}
          </div>
        </div>
      </div>
    </section>
  )
}

/** The four areas settings is split into, in reading order. */
type SectionId = 'projects' | 'behavior' | 'service' | 'notifications'

const SECTIONS: {
  id: SectionId
  icon: ComponentType<{ size?: number; strokeWidth?: number; className?: string }>
  label: MessageKey
  hint: MessageKey
}[] = [
  { id: 'projects', icon: Folder, label: 'settings.section.projects', hint: 'settings.section.projectsHint' },
  { id: 'behavior', icon: SlidersHorizontal, label: 'settings.section.behavior', hint: 'settings.section.behaviorHint' },
  { id: 'service', icon: Server, label: 'settings.section.service', hint: 'settings.section.serviceHint' },
  { id: 'notifications', icon: BellRing, label: 'settings.section.notifications', hint: 'settings.section.notificationsHint' },
]
/**
 * Project registry settings.
 *
 * A project is a local folder plus the DSH profile whose credentials and
 * plugins belong to it. The project switcher lives in the title bar; this is
 * where projects are made, renamed, rebound and removed.
 */
function ProjectsSection() {
  const roster = useProjects((state) => state.roster)
  const working = useProjects((state) => state.working)
  const error = useProjects((state) => state.error)
  const refresh = useProjects((state) => state.refresh)
  const remove = useProjects((state) => state.remove)
  const rename = useProjects((state) => state.rename)
  const bindProfile = useProjects((state) => state.bindProfile)
  const profiles = useProfiles((state) => state.roster)
  const refreshProfiles = useProfiles((state) => state.refresh)
  const [editing, setEditing] = useState<string | null>(null)
  const [draft, setDraft] = useState('')

  useEffect(() => {
    void refresh()
    void refreshProfiles()
  }, [refresh, refreshProfiles])

  const choose = async () => {
    const chosen = await openDialog({
      title: t('project.new'),
      directory: true,
      multiple: false,
    })
    if (typeof chosen !== 'string') return
    await addProjectWorkspace(chosen)
  }

  const beginRename = (id: string, current: string) => {
    setEditing(id)
    setDraft(current)
  }

  const commitRename = async (id: string) => {
    const clean = draft.trim()
    if (clean) await rename(id, clean)
    setEditing(null)
  }

  const confirmRemove = async (id: string, name: string) => {
    const taken = await ask({
      title: t('project.remove'),
      body: `${name}`,
      subject: name,
      confirm: t('project.remove'),
      tone: 'danger',
    })
    if (!taken) return
    await remove(id)
  }

  return (
    <div className="overflow-hidden rounded-panel border border-line bg-canvas-deep/50">
      <div className="flex items-center justify-between border-b border-line px-4 py-3">
        <div>
          <h3 className="text-[12.5px] font-medium text-text">{t('project.manage')}</h3>
          <p className="mt-0.5 text-[11.5px] text-faint">
            {t('check.project')} · {roster?.projects.length ?? 0}
          </p>
        </div>
        <Button variant="secondary" onClick={() => void choose()} disabled={working !== null}>
          <FolderPlus size={13} strokeWidth={2.1} />
          {t('project.new')}
        </Button>
      </div>

      {error && (
        <p className="selectable border-b border-danger/30 bg-danger/10 px-4 py-2 text-[11.5px] leading-relaxed text-danger">
          {error}
        </p>
      )}

      {(roster?.projects ?? []).length === 0 ? (
        <p className="px-4 py-6 text-center text-[12px] text-faint">{t('project.new')}</p>
      ) : (
        <ul className="divide-y divide-line">
          {(roster?.projects ?? []).map((project) => (
            <li key={project.id} className="px-4 py-3">
              <div className="flex items-center gap-2">
                <button
                  type="button"
                  data-hint={project.path}
                  onClick={() => void switchProject(project.id)}
                  className="min-w-0 flex-1 truncate text-left text-[12.5px] font-medium text-text transition-colors duration-100 hover:text-brand"
                >
                  <span className="truncate">{project.name}</span>
                  <span className="ml-2 text-[11px] text-faint">{leaf(project.path)}</span>
                </button>

                <Button
                  variant="ghost"
                  aria-label={t('project.rename')}
                  data-hint={t('project.rename')}
                  onClick={() => beginRename(project.id, project.name)}
                >
                  <Pencil size={13} strokeWidth={2.1} />
                </Button>

                <Button
                  variant="danger"
                  aria-label={t('project.remove')}
                  data-hint={t('project.remove')}
                  disabled={working !== null || (roster?.projects.length ?? 0) <= 1}
                  onClick={() => void confirmRemove(project.id, project.name)}
                >
                  <Trash2 size={13} strokeWidth={2.1} />
                </Button>
              </div>

              {editing === project.id && (
                <div className="mt-2 flex items-center gap-2">
                  <input
                    autoFocus
                    value={draft}
                    aria-label={t('project.rename')}
                    onChange={(event) => setDraft(event.target.value)}
                    onBlur={() => void commitRename(project.id)}
                    onKeyDown={(event) => {
                      if (event.key === 'Enter') event.currentTarget.blur()
                      if (event.key === 'Escape') {
                        setEditing(null)
                      }
                    }}
                    className="h-[30px] min-w-0 flex-1 rounded-control border border-line-strong bg-surface-2 px-2.5 text-[12px] text-text outline-none focus:border-brand"
                  />
                </div>
              )}

              <label className="mt-2 flex items-center gap-2 text-[11.5px] text-faint">
                <span className="shrink-0">{t('project.profile')}</span>
                <Select
                  value={project.profile}
                  label={t('project.profile')}
                  disabled={working !== null}
                  wrapperClassName="min-w-0 flex-1"
                  className="h-[28px]"
                  onChange={(event) => void bindProfile(project.id, event.target.value)}
                >
                  {(profiles?.profiles ?? []).map((profile) => (
                    <option key={profile.name} value={profile.name}>
                      {profile.name}
                    </option>
                  ))}
                </Select>
              </label>
            </li>
          ))}
        </ul>
      )}
    </div>
  )
}

/** Keep the end of a path, which is the part that identifies it. */
function leaf(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean)
  return parts.length > 0 ? (parts[parts.length - 1] ?? path) : path
}

/**
 * The combination itself, which is also the control that changes it.
 *
 * Pressing the keys is the only way to say which keys, so the display and the
 * recorder are one button rather than a readout with a "change" beside it — the
 * thing on screen is the shortcut, and pressing it is how you replace it.
 */
interface RecorderProps {
  recording: boolean
  onRecording: (recording: boolean) => void
}

function Recorder({ recording, onRecording: setRecording }: RecorderProps) {
  const state = useStartup((store) => store.state)
  const busy = useStartup((store) => store.busy)
  const setShortcut = useStartup((store) => store.setShortcut)

  const held = state?.shortcut ?? null
  const keys = held ? spellCombination(held, isMac) : []

  const capture = (event: KeyboardEvent<HTMLButtonElement>) => {
    // Held before anything is read. While this button has the keyboard it has
    // all of it: the window listeners in `App.tsx` and `native.ts` would
    // otherwise answer the very combination being recorded, and the webview
    // would act on whatever it recognises.
    event.preventDefault()
    event.stopPropagation()
    if (event.repeat) return

    // Checked ahead of the rest so Ctrl+Escape is a way out and not a choice.
    if (event.code === 'Escape') {
      setRecording(false)
      return
    }

    // Null while only modifiers are down, and null for a combination Rust would
    // refuse — either way there is nothing yet, so the button keeps listening.
    const combination = readCombination(event)
    if (!combination) return

    setRecording(false)
    void setShortcut(combination)
  }

  return (
    <div className="flex items-center gap-1.5">
      <button
        type="button"
        disabled={busy || state === null}
        data-hint={recording ? undefined : t('settings.record')}
        onClick={(event) => {
          // Focused by hand because a click does not focus a button on every
          // platform, and a recorder that is not focused hears nothing.
          event.currentTarget.focus()
          setRecording(true)
        }}
        onKeyDown={recording ? capture : undefined}
        onBlur={() => setRecording(false)}
        className={[
          'flex h-[30px] min-w-[152px] items-center justify-center gap-1 rounded-control border px-2.5 text-[12px] transition duration-100 ease-[var(--ease-out-soft)] select-none disabled:opacity-40',
          recording
            ? 'border-brand bg-brand/10 text-brand'
            : 'border-line-strong bg-surface-2 text-text enabled:hover:brightness-[1.15] enabled:active:brightness-95',
        ].join(' ')}
      >
        {recording ? (
          t('settings.recording')
        ) : keys.length > 0 ? (
          <Chips keys={keys} />
        ) : (
          <span className="text-muted">{t('settings.none')}</span>
        )}
      </button>

      {!recording &&
        (held ? (
          <Button
            variant="ghost"
            disabled={busy}
            aria-label={t('settings.clear')}
            data-hint={t('settings.clear')}
            onClick={() => void setShortcut(null)}
          >
            <X size={13} strokeWidth={2.3} aria-hidden="true" />
          </Button>
        ) : (
          // Offered rather than applied. Somebody who wants a global key almost
          // never has an opinion about which one, and inventing a combination
          // that nothing else on the machine has taken is the tedious half of
          // this setting — but taking a key without being asked is the rude half.
          state && (
            <Button
              variant="ghost"
              disabled={busy}
              aria-label={t('settings.suggest', {
                keys: spellCombination(state.suggested, isMac).join(' '),
              })}
              data-hint={t('settings.suggest', {
                keys: spellCombination(state.suggested, isMac).join(' '),
              })}
              onClick={() => void setShortcut(state.suggested)}
            >
              <Chips keys={spellCombination(state.suggested, isMac)} />
            </Button>
          )
        ))}
    </div>
  )
}

interface PortFieldProps {
  value: number | null
  disabled: boolean
  onSave: (port: number | null) => void
}

function PortField({ value, disabled, onSave }: PortFieldProps) {
  const [draft, setDraft] = useState(value?.toString() ?? '')

  const save = () => {
    const normalized = draft.trim()
    if (normalized === '') {
      if (value !== null) onSave(null)
      return
    }
    const port = Number(normalized)
    if (Number.isInteger(port) && port >= 1_024 && port <= 65_535 && port !== value) {
      onSave(port)
    }
  }

  return (
    <input
      type="number"
      min={1024}
      max={65535}
      step={1}
      inputMode="numeric"
      value={draft}
      placeholder={t('settings.harnessPortAuto')}
      aria-label={t('settings.harnessPort')}
      disabled={disabled}
      onChange={(event) => setDraft(event.target.value)}
      onBlur={save}
      onKeyDown={(event) => {
        if (event.key === 'Enter') event.currentTarget.blur()
        if (event.key === 'Escape') {
          setDraft(value?.toString() ?? '')
          event.currentTarget.blur()
        }
      }}
      className="h-[30px] w-[112px] rounded-control border border-line-strong bg-surface-2 px-2.5 text-right text-[11.5px] text-text outline-none placeholder:text-faint focus:border-brand disabled:opacity-40"
    />
  )
}

/** A combination, drawn as keys rather than spelled out with plus signs. */
function Chips({ keys }: { keys: string[] }) {
  return (
    <>
      {keys.map((key, index) => (
        <kbd
          // Not the key itself: a combination can repeat one, and a duplicate
          // React key is a rendering bug rather than a display one.
          key={`${index}-${key}`}
          className="grid h-[17px] min-w-[18px] place-items-center rounded-[4px] border border-line bg-canvas px-1 font-sans text-[10.5px] text-text"
        >
          {key}
        </kbd>
      ))}
    </>
  )
}

interface RowProps {
  icon: ComponentType<{ size?: number; strokeWidth?: number; className?: string }>
  label: string
  hint: string
  /** Shown under the hint when there is something to say about this setting. */
  note?: ReactNode
  children: ReactNode
}

function Row({ icon: Icon, label, hint, note, children }: RowProps) {
  return (
    <div className="flex items-start gap-3.5 px-4 py-3.5">
      <Icon size={15} strokeWidth={2} className="mt-[3px] shrink-0 text-faint" />
      <div className="flex min-w-0 flex-1 flex-col gap-1">
        <span className="text-[12.5px] font-medium text-text">{label}</span>
        <p className="text-[11.5px] leading-relaxed text-faint">{hint}</p>
        {note && <div className="mt-0.5 text-[11.5px] leading-relaxed">{note}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2 pt-[3px]">{children}</div>
    </div>
  )
}
