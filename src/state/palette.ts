/**
 * Whether the command palette is up, and what was last run in it.
 *
 * A store rather than a flag in the window component, for the same reason the
 * context menu is one: the key that opens it is listened for at the window, the
 * thing that draws it sits at the root of the layout, and anything else that
 * wants to offer a way in — a button in a strip, an empty state saying "try
 * Ctrl+K" — should be able to open it without a callback threaded down to it.
 *
 * The recents are ids, not commands: the roster the palette draws is rebuilt
 * every time it opens, so a stored command object would go stale the moment a
 * profile was renamed or a plugin uninstalled. The palette resolves the ids
 * against the live list when it draws and quietly drops the ones that no
 * longer answer.
 */
import { create } from 'zustand'

const RECENTS_KEY = 'harnesslite.palette.recents'

/** Enough to be a memory, few enough that every row is still one keystroke's neighbour. */
const RECENTS_MAX = 6

interface PaletteState {
  open: boolean
  /** Command ids, newest first. May contain ids the current roster no longer has. */
  recents: string[]
  recordRecent: (id: string) => void
  show: () => void
  hide: () => void
  toggle: () => void
}

const readRecents = (): string[] => {
  try {
    const raw = window.localStorage.getItem(RECENTS_KEY)
    const parsed: unknown = raw === null ? [] : JSON.parse(raw)
    return Array.isArray(parsed) ? parsed.filter((id): id is string => typeof id === 'string') : []
  } catch {
    return []
  }
}

export const usePalette = create<PaletteState>((set, get) => ({
  open: false,
  recents: readRecents(),
  recordRecent: (id) => {
    const recents = [id, ...get().recents.filter((seen) => seen !== id)].slice(0, RECENTS_MAX)
    set({ recents })
    try {
      window.localStorage.setItem(RECENTS_KEY, JSON.stringify(recents))
    } catch {
      // Private modes refuse storage; the session still keeps its list.
    }
  },
  show: () => set({ open: true }),
  hide: () => set({ open: false }),
  toggle: () => set({ open: !get().open }),
}))
