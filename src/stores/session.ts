// Per-tab session state living OUTSIDE React, so a tab can be unmounted
// (sleep) and re-mounted (awake) without losing files, params or history.
//
// Each session is a zustand store wrapped with zundo's temporal middleware.
// Only { files, params, output, lastAction } are tracked; selection, view
// size and scroll are UI-only and never create history steps.
import { temporal, type TemporalState } from 'zundo'
import { createStore, useStore, type StoreApi } from 'zustand'
import { shallow } from 'zustand/shallow'

import type { ImportedFile, OutputSettings } from '@/lib/ipc/types'

export const HISTORY_LIMIT = 100
export const COALESCE_MS = 400

export type ViewSize = 'small' | 'medium' | 'large'

/** Identifies the action that produced a history step (used for Undo/Redo tooltips). */
export type HistoryActionKey =
  | 'addFiles'
  | 'removeFiles'
  | 'clearFiles'
  | 'reorderFiles'
  | 'changeParams'
  | 'resetParams'
  | 'applyPreset'
  | 'changeOutput'

export interface LastAction {
  key: HistoryActionKey
  count?: number
}

export type Params = Record<string, unknown>

/** The part of the session that undo/redo tracks. */
export interface SessionData {
  files: ImportedFile[]
  params: Params
  output: OutputSettings
  lastAction: LastAction | null
}

export interface SessionUi {
  selectedIds: string[]
  /** Anchor for Shift+click range selection. */
  anchorId: string | null
  viewSize: ViewSize
  scrollTop: number
  /** Misc. per-tab UI flags (collapsed groups, size links, preview view). Never tracked. */
  uiFlags: Record<string, boolean>
}

/** A file that now lives at `file.path` instead of `from`. */
export interface FileReplacement {
  from: string
  file: ImportedFile
}

export interface SetOptions {
  /** Merge consecutive changes (slider drag, typing) into ONE history step. */
  coalesce?: boolean
}

export interface SessionActions {
  /** Appends files not already present (dedupe by id). Returns how many were added. */
  addFiles: (files: ImportedFile[]) => number
  /** Removes files by id. Returns how many were removed. */
  removeFiles: (ids: string[]) => number
  clearFiles: () => number
  /** Moves the file at `from` to index `to`. */
  reorderFiles: (from: number, to: number) => void
  /**
   * Swaps in files whose path changed on disk (e.g. after a rename), matched by
   * their old path. This is NOT an undo step: the disk change is not undoable
   * here, so the new paths are also written into the undo/redo history (undo
   * never brings back paths that no longer exist). Selection follows the files.
   * Returns how many files of the current list were replaced.
   */
  replaceFiles: (updates: FileReplacement[]) => number
  setParams: (patch: Params, options?: SetOptions) => void
  /** Closes an open coalescing group (call on pointer-up / blur). */
  commitParams: () => void
  resetParams: () => void
  applyPreset: (params: Params) => void
  setOutput: (patch: Partial<OutputSettings>, options?: SetOptions) => void

  select: (id: string, mode?: 'replace' | 'toggle' | 'range') => void
  setSelection: (ids: string[]) => void
  selectAll: () => void
  clearSelection: () => void
  setViewSize: (size: ViewSize) => void
  setScrollTop: (top: number) => void
  setUiFlag: (key: string, value: boolean) => void

  undo: () => void
  redo: () => void
}

export type SessionState = SessionData & SessionUi & SessionActions & { readonly tabId: string; readonly defaults: Params }

export type SessionStore = StoreApi<SessionState> & { temporal: StoreApi<TemporalState<SessionData>> }

export const defaultOutputSettings = (): OutputSettings => ({
  mode: { kind: 'suffix', suffix: '_opt' },
  format: 'keep',
  pngCompression: 'default',
  jpgQuality: 90,
  optimizePng: false,
  conflict: 'autoRename',
})

export interface SessionInit {
  params?: Params
  files?: ImportedFile[]
  output?: OutputSettings
  viewSize?: ViewSize
}

function deepEqual(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true
  if (typeof a !== 'object' || typeof b !== 'object' || a === null || b === null) return false
  if (Array.isArray(a) !== Array.isArray(b)) return false
  const ka = Object.keys(a)
  const kb = Object.keys(b)
  if (ka.length !== kb.length) return false
  return ka.every((k) => deepEqual((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]))
}

function patchChanges(current: object, patch: object): boolean {
  return Object.entries(patch).some(([k, v]) => !deepEqual((current as Record<string, unknown>)[k], v))
}

export function createSessionStore(tabId: string, init: SessionInit = {}): SessionStore {
  const defaults = structuredClone(init.params ?? {})
  // Open coalescing group: history is paused after its first change.
  let group: { key: string; timer: ReturnType<typeof setTimeout> } | null = null

  const store = createStore<SessionState>()(
    temporal(
      (set, get, api) => {
        const history = () => (api as unknown as SessionStore).temporal.getState()

        const commit = () => {
          if (!group) return
          clearTimeout(group.timer)
          group = null
          history().resume()
        }

        const coalesced = (key: string, apply: () => void) => {
          if (group && group.key !== key) commit()
          if (!group) {
            apply() // recorded: the pre-change state becomes one history step
            history().pause()
            group = { key, timer: setTimeout(commit, COALESCE_MS) }
          } else {
            apply() // paused: folded into the open step
            clearTimeout(group.timer)
            group.timer = setTimeout(commit, COALESCE_MS)
          }
        }

        const pruneSelection = () => {
          const { files, selectedIds, anchorId } = get()
          const ids = new Set(files.map((f) => f.id))
          const kept = selectedIds.filter((id) => ids.has(id))
          if (kept.length !== selectedIds.length || (anchorId && !ids.has(anchorId))) {
            set({ selectedIds: kept, anchorId: anchorId && ids.has(anchorId) ? anchorId : null })
          }
        }

        return {
          tabId,
          defaults,
          files: init.files ?? [],
          params: structuredClone(defaults),
          output: init.output ?? defaultOutputSettings(),
          lastAction: null,
          selectedIds: [],
          anchorId: null,
          viewSize: init.viewSize ?? 'medium',
          scrollTop: 0,
          uiFlags: {},

          addFiles: (incoming) => {
            commit()
            const seen = new Set(get().files.map((f) => f.id))
            const fresh: ImportedFile[] = []
            for (const f of incoming) {
              if (seen.has(f.id)) continue
              seen.add(f.id)
              fresh.push(f)
            }
            if (fresh.length === 0) return 0
            set({ files: [...get().files, ...fresh], lastAction: { key: 'addFiles', count: fresh.length } })
            return fresh.length
          },

          removeFiles: (ids) => {
            commit()
            const drop = new Set(ids)
            const files = get().files
            const kept = files.filter((f) => !drop.has(f.id))
            const removed = files.length - kept.length
            if (removed === 0) return 0
            set({ files: kept, lastAction: { key: 'removeFiles', count: removed } })
            pruneSelection()
            return removed
          },

          clearFiles: () => {
            commit()
            const count = get().files.length
            if (count === 0) return 0
            set({ files: [], lastAction: { key: 'clearFiles', count } })
            pruneSelection()
            return count
          },

          reorderFiles: (from, to) => {
            commit()
            const files = [...get().files]
            if (from === to || from < 0 || from >= files.length) return
            const [moved] = files.splice(from, 1)
            files.splice(Math.max(0, Math.min(to, files.length)), 0, moved)
            set({ files, lastAction: { key: 'reorderFiles', count: 1 } })
          },

          replaceFiles: (updates) => {
            commit()
            if (updates.length === 0) return 0
            const byPath = new Map(updates.map((u) => [u.from, u.file]))
            const remap = (files: ImportedFile[]) => {
              let changed = false
              const out = files.map((f) => {
                const next = byPath.get(f.path)
                if (!next) return f
                changed = true
                return next
              })
              return changed ? out : files
            }
            const { files, selectedIds, anchorId } = get()
            const next = remap(files)
            const ids = new Map<string, string>()
            files.forEach((f, i) => {
              if (next[i] !== f) ids.set(f.id, next[i].id)
            })
            if (ids.size === 0) return 0
            const temporalApi = (api as unknown as SessionStore).temporal
            const h = temporalApi.getState()
            const tracking = h.isTracking
            h.pause()
            set({
              files: next,
              selectedIds: selectedIds.map((id) => ids.get(id) ?? id),
              anchorId: anchorId ? (ids.get(anchorId) ?? anchorId) : null,
            })
            if (tracking) h.resume()
            const rewrite = (states: Partial<SessionData>[]) =>
              states.map((s) => {
                if (!s.files) return s
                const f = remap(s.files)
                return f === s.files ? s : { ...s, files: f }
              })
            temporalApi.setState({ pastStates: rewrite(h.pastStates), futureStates: rewrite(h.futureStates) })
            return ids.size
          },

          setParams: (patch, options) => {
            if (!patchChanges(get().params, patch)) return
            const apply = () => set({ params: { ...get().params, ...patch }, lastAction: { key: 'changeParams' } })
            if (options?.coalesce) coalesced(`params:${Object.keys(patch).sort().join('|')}`, apply)
            else {
              commit()
              apply()
            }
          },

          commitParams: commit,

          resetParams: () => {
            commit()
            if (deepEqual(get().params, defaults)) return
            set({ params: structuredClone(defaults), lastAction: { key: 'resetParams' } })
          },

          applyPreset: (params) => {
            commit()
            const next = { ...structuredClone(defaults), ...structuredClone(params) }
            if (deepEqual(get().params, next)) return
            set({ params: next, lastAction: { key: 'applyPreset' } })
          },

          setOutput: (patch, options) => {
            if (!patchChanges(get().output, patch)) return
            const apply = () => set({ output: { ...get().output, ...patch }, lastAction: { key: 'changeOutput' } })
            if (options?.coalesce) coalesced(`output:${Object.keys(patch).sort().join('|')}`, apply)
            else {
              commit()
              apply()
            }
          },

          select: (id, mode = 'replace') => {
            const { selectedIds, anchorId, files } = get()
            if (mode === 'toggle') {
              const has = selectedIds.includes(id)
              set({ selectedIds: has ? selectedIds.filter((x) => x !== id) : [...selectedIds, id], anchorId: id })
            } else if (mode === 'range' && anchorId) {
              const a = files.findIndex((f) => f.id === anchorId)
              const b = files.findIndex((f) => f.id === id)
              if (a < 0 || b < 0) {
                set({ selectedIds: [id], anchorId: id })
                return
              }
              const [lo, hi] = a < b ? [a, b] : [b, a]
              set({ selectedIds: files.slice(lo, hi + 1).map((f) => f.id) })
            } else {
              set({ selectedIds: [id], anchorId: id })
            }
          },
          setSelection: (ids) => set({ selectedIds: ids, anchorId: ids.at(-1) ?? null }),
          selectAll: () => set({ selectedIds: get().files.map((f) => f.id) }),
          clearSelection: () => set({ selectedIds: [], anchorId: null }),
          setViewSize: (viewSize) => set({ viewSize }),
          setScrollTop: (scrollTop) => set({ scrollTop }),
          setUiFlag: (key, value) => set({ uiFlags: { ...get().uiFlags, [key]: value } }),

          undo: () => {
            commit()
            history().undo()
            pruneSelection()
          },
          redo: () => {
            commit()
            history().redo()
            pruneSelection()
          },
        }
      },
      {
        limit: HISTORY_LIMIT,
        partialize: (s): SessionData => ({ files: s.files, params: s.params, output: s.output, lastAction: s.lastAction }),
        // UI-only changes (selection, view size, scroll) leave tracked refs untouched -> no step.
        equality: (a, b) => shallow(a, b),
      },
    ),
  )
  return store as unknown as SessionStore
}

// ---------------------------------------------------------------------------
// Registry: tabId -> session store, outside React.

const sessions = new Map<string, SessionStore>()

export function createSession(tabId: string, init?: SessionInit): SessionStore {
  const existing = sessions.get(tabId)
  if (existing) return existing
  const store = createSessionStore(tabId, init)
  sessions.set(tabId, store)
  return store
}

export function getSession(tabId: string): SessionStore | undefined {
  return sessions.get(tabId)
}

/** Returns the session, creating an empty one if needed (never throws in render). */
export function requireSession(tabId: string): SessionStore {
  return sessions.get(tabId) ?? createSession(tabId)
}

export function deleteSession(tabId: string): void {
  const store = sessions.get(tabId)
  if (!store) return
  store.getState().commitParams()
  store.temporal.getState().clear()
  sessions.delete(tabId)
}

export function hasSession(tabId: string): boolean {
  return sessions.has(tabId)
}

/** Test helper: drop every session. */
export function resetSessions(): void {
  for (const id of [...sessions.keys()]) deleteSession(id)
}

// ---------------------------------------------------------------------------
// React hooks

export function useSession<T>(tabId: string, selector: (state: SessionState) => T): T {
  return useStore(requireSession(tabId), selector)
}

export interface HistoryInfo {
  canUndo: boolean
  canRedo: boolean
  /** Action that Undo would revert. */
  undoAction: LastAction | null
  /** Action that Redo would re-apply. */
  redoAction: LastAction | null
  pastCount: number
  futureCount: number
}

export function useSessionHistory(tabId: string): HistoryInfo {
  const store = requireSession(tabId)
  const pastCount = useStore(store.temporal, (s) => s.pastStates.length)
  const futureCount = useStore(store.temporal, (s) => s.futureStates.length)
  const redoAction = useStore(store.temporal, (s) => s.futureStates.at(-1)?.lastAction ?? null)
  const current = useStore(store, (s) => s.lastAction)
  return {
    canUndo: pastCount > 0,
    canRedo: futureCount > 0,
    undoAction: pastCount > 0 ? current : null,
    redoAction: futureCount > 0 ? redoAction : null,
    pastCount,
    futureCount,
  }
}
