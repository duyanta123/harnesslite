/**
 * Every project on the machine, and which one this window is pointed at.
 *
 * A project is a local folder plus a DSH profile. The list always comes back
 * from Rust rather than being patched locally, for the same reason profiles do:
 * a project's path is validated on disk, and only the native side can decide
 * whether a new folder is safe to run agent work in.
 */
import { create } from 'zustand'

import { describe } from '@/lib/errors'
import { t } from '@/lib/i18n'
import * as ipc from '@/lib/ipc'
import type { ProjectRoster } from '@/lib/ipc'
import { ask } from '@/state/dialog'
import { reportFailure } from '@/state/failure'
import { useHarness } from '@/state/harness'

interface ProjectStore {
  /** Null until the first read lands. */
  roster: ProjectRoster | null
  /** Which project a change is running against, or null when idle. */
  working: string | null
  /** Last thing that went wrong, or null when idle. */
  error: string | null

  refresh: () => Promise<void>
  /** Record the choice. Coordinated by `switchProject`, not called on its own. */
  select: (id: string) => Promise<boolean>
  add: (path: string, name?: string, profile?: string) => Promise<boolean>
  remove: (id: string) => Promise<boolean>
  rename: (id: string, name: string) => Promise<boolean>
  bindProfile: (id: string, profile: string) => Promise<boolean>
  /** Drop the error, so a reopened manager starts clean. */
  settle: () => void
}

type Write = (partial: Partial<ProjectStore>) => void

/** One mutation at a time; the reply replaces the roster. */
const change = async (
  set: Write,
  get: () => ProjectStore,
  subject: string,
  run: () => Promise<ProjectRoster>,
): Promise<boolean> => {
  if (get().working !== null) return false

  set({ working: subject, error: null })
  try {
    const roster = await run()
    set({ roster })
    // Project changes can create a profile, and the title-bar profile chip has
    // to stay in step with the directory that now exists.
    void ipc.announce('projects')
    void ipc.announce('profiles')
    return true
  } catch (cause) {
    set({ error: reportFailure(cause) })
    return false
  } finally {
    set({ working: null })
  }
}

export const useProjects = create<ProjectStore>((set, get) => ({
  roster: null,
  working: null,
  error: null,

  refresh: async () => {
    if (get().working !== null) return
    try {
      set({ roster: await ipc.projectsList() })
    } catch (cause) {
      set({ error: describe(cause) })
    }
  },

  select: (id) => change(set, get, id, () => ipc.projectsSelect(id)),

  add: (path, name, profile) => change(set, get, path, () => ipc.projectsAdd(path, name, profile)),

  remove: (id) => change(set, get, id, () => ipc.projectsRemove(id)),

  rename: (id, name) => change(set, get, id, () => ipc.projectsRename(id, name)),

  bindProfile: (id, profile) => change(set, get, id, () => ipc.projectsBindProfile(id, profile)),

  settle: () => set({ error: null }),
}))

/**
 * Point this window at another project.
 *
 * Selecting a project writes two native choices: the project registry and the
 * DSH profile binding. A running harness keeps working the old project until it
 * is restarted, so the restart is offered rather than taken — ending live
 * sessions is the user's call.
 */
export async function switchProject(id: string): Promise<boolean> {
  const store = useProjects.getState()
  if (store.working !== null) return false
  if (store.roster?.selected !== id) {
    if (!(await store.select(id))) return false
  }

  try {
    await useHarness.getState().inspect()
  } catch {
    return false
  }

  const { phase } = useHarness.getState().status
  if (phase === 'stopped' || phase === 'failed') return true

  const project = useProjects.getState().roster?.projects.find((candidate) => candidate.id === id)
  const taken = await ask({
    title: t('project.restartTitle'),
    body: t('project.restartBody'),
    subject: project?.name ?? id,
    confirm: t('project.restartConfirm'),
    tone: 'brand',
  })
  if (!taken) return true

  await useHarness.getState().stop()
  await useHarness.getState().start()
  return true
}

/**
 * Pick a native folder, admit it as a project, and offer the restart needed to
 * apply it. Used by the status-bar folder control and native drag-drop.
 */
export async function addProjectWorkspace(path: string): Promise<boolean> {
  const store = useProjects.getState()
  if (store.working !== null) return false

  useProjects.setState({ working: path, error: null })
  try {
    try {
      const roster = await ipc.projectsAdd(path)
      useProjects.setState({ roster, error: null })
      void ipc.announce('projects')
      void ipc.announce('profiles')
    } catch (cause) {
      useProjects.setState({ error: reportFailure(cause) })
      return false
    }

    try {
      await useHarness.getState().inspect()
    } catch {
      return false
    }

    const { phase } = useHarness.getState().status
    if (phase === 'stopped' || phase === 'failed') return true

    const taken = await ask({
      title: t('project.restartTitle'),
      body: t('project.restartBody'),
      subject: path,
      confirm: t('project.restartConfirm'),
      tone: 'brand',
    })
    if (!taken) return true

    await useHarness.getState().stop()
    await useHarness.getState().start()
    return true
  } finally {
    useProjects.setState({ working: null })
  }
}

/**
 * Follow what the other windows do to the projects.
 *
 * Subscribed for the lifetime of the window, because the title-bar project
 * switcher is never closed and has to stay true while another window adds or
 * removes a project.
 */
export const subscribeToProjects = (): Promise<() => void> =>
  ipc.onSharedChange((subject) => {
    if (subject !== 'projects') return
    void useProjects.getState().refresh()
  })
