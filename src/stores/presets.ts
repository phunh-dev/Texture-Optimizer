import { create } from 'zustand'

import { loadValue, PRESETS_FILE, saveValue } from '@/lib/persist'

import type { Params } from './session'

export interface Preset {
  name: string
  params: Params
  updatedAt: number
}

interface PresetsState {
  /** toolId -> presets (sorted by name). */
  byTool: Record<string, Preset[]>
  loaded: Record<string, boolean>
  load: (toolId: string) => Promise<Preset[]>
  save: (toolId: string, name: string, params: Params) => Promise<void>
  remove: (toolId: string, name: string) => Promise<void>
}

const sortByName = (list: Preset[]) => [...list].sort((a, b) => a.name.localeCompare(b.name))

export const usePresets = create<PresetsState>()((set, get) => ({
  byTool: {},
  loaded: {},

  load: async (toolId) => {
    if (get().loaded[toolId]) return get().byTool[toolId] ?? []
    const stored = await loadValue<Preset[]>(PRESETS_FILE, toolId)
    const list = Array.isArray(stored) ? sortByName(stored.filter((p) => p && typeof p.name === 'string')) : []
    set((s) => ({ byTool: { ...s.byTool, [toolId]: list }, loaded: { ...s.loaded, [toolId]: true } }))
    return list
  },

  save: async (toolId, name, params) => {
    await get().load(toolId)
    const trimmed = name.trim()
    if (!trimmed) return
    const others = (get().byTool[toolId] ?? []).filter((p) => p.name !== trimmed)
    const list = sortByName([...others, { name: trimmed, params: structuredClone(params), updatedAt: Date.now() }])
    set((s) => ({ byTool: { ...s.byTool, [toolId]: list } }))
    await saveValue(PRESETS_FILE, toolId, list)
  },

  remove: async (toolId, name) => {
    await get().load(toolId)
    const list = (get().byTool[toolId] ?? []).filter((p) => p.name !== name)
    set((s) => ({ byTool: { ...s.byTool, [toolId]: list } }))
    await saveValue(PRESETS_FILE, toolId, list)
  },
}))
