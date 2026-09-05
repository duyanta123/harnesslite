/**
 * Where the settings pane is looking, kept between visits.
 *
 * A store rather than pane-local state, for two reasons: the pane unmounts
 * when its sheet closes and would otherwise forget, and other surfaces need to
 * name the section they want — the project chip's menu opens settings pointed
 * at the projects list, not at wherever the pane happened to be left.
 */
import { create } from 'zustand'

export type SettingsSection = 'projects' | 'behavior' | 'service' | 'notifications' | 'plugins'

interface SettingsNavState {
  section: SettingsSection
  visit: (section: SettingsSection) => void
}

export const useSettingsNav = create<SettingsNavState>((set) => ({
  section: 'projects',
  visit: (section) => set({ section }),
}))
