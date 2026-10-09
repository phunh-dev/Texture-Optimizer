import { create } from 'zustand'

import { inTauri } from '@/lib/env'
import { releaseSession } from '@/lib/ipc'
import { createId } from '@/lib/utils'
import { getTool, type ToolId } from '@/tabs/registry'

import { useJobs } from './jobs'
import { createSession, deleteSession } from './session'

export interface TabInfo {
  id: string
  toolId: ToolId
  /** n-th open tab of the same tool (1-based) -> "Resize 2". */
  ordinal: number
}

interface TabsState {
  tabs: TabInfo[]
  activeTabId: string | null
  /** Opens a new tab of `toolId`, activates it and returns its id. */
  openTab: (toolId: ToolId) => string
  closeTab: (id: string) => void
  activateTab: (id: string) => void
  /** Activates the next (+1) / previous (-1) tab, wrapping around. */
  cycleTab: (direction: 1 | -1) => void
  moveTab: (from: number, to: number) => void
}

/** Tell the backend to drop decoded caches of a sleeping/closed tab (no-op outside Tauri). */
export function releaseTabSession(tabId: string): void {
  if (!inTauri()) return
  try {
    void releaseSession(tabId).catch(() => undefined)
  } catch {
    // ignore: backend not ready
  }
}

export const useTabs = create<TabsState>()((set, get) => ({
  tabs: [],
  activeTabId: null,

  openTab: (toolId) => {
    const tool = getTool(toolId)
    const id = createId('tab')
    const ordinal = Math.max(0, ...get().tabs.filter((t) => t.toolId === toolId).map((t) => t.ordinal)) + 1
    createSession(id, { params: tool.defaultParams() })
    const previous = get().activeTabId
    set({ tabs: [...get().tabs, { id, toolId, ordinal }], activeTabId: id })
    if (previous) releaseTabSession(previous)
    return id
  },

  closeTab: (id) => {
    const { tabs, activeTabId } = get()
    const index = tabs.findIndex((t) => t.id === id)
    if (index < 0) return
    const next = tabs.filter((t) => t.id !== id)
    let nextActive = activeTabId
    if (activeTabId === id) nextActive = (next[index] ?? next[index - 1])?.id ?? null
    set({ tabs: next, activeTabId: nextActive })
    useJobs.getState().forgetTab(id, { cancel: true })
    deleteSession(id)
    releaseTabSession(id)
  },

  activateTab: (id) => {
    const previous = get().activeTabId
    if (previous === id || !get().tabs.some((t) => t.id === id)) return
    set({ activeTabId: id })
    if (previous) releaseTabSession(previous)
  },

  cycleTab: (direction) => {
    const { tabs, activeTabId } = get()
    if (tabs.length < 2) return
    const index = tabs.findIndex((t) => t.id === activeTabId)
    const next = tabs[(index + direction + tabs.length) % tabs.length]
    get().activateTab(next.id)
  },

  moveTab: (from, to) => {
    const tabs = [...get().tabs]
    if (from === to || from < 0 || from >= tabs.length || to < 0 || to >= tabs.length) return
    const [moved] = tabs.splice(from, 1)
    tabs.splice(to, 0, moved)
    set({ tabs })
  },
}))

export function useActiveTab(): TabInfo | null {
  return useTabs((s) => s.tabs.find((t) => t.id === s.activeTabId) ?? null)
}

export function getTabInfo(tabId: string): TabInfo | undefined {
  return useTabs.getState().tabs.find((t) => t.id === tabId)
}
