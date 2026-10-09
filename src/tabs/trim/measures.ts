// Trimmed sizes depend on pixel content, so they are measured (via previewOp)
// rather than predicted: from the live preview, or on demand for every file.
// Kept outside React so measurements survive tab sleep; keyed per tab + file.
import { create } from 'zustand'

import { toAppError } from '@/lib/errors'
import { previewOp } from '@/lib/ipc'
import type { ImportedFile, OpRequest } from '@/lib/ipc/types'

import type { SizeResult } from '../_imageOps/size'
import { isTrimMeta } from './schema'

export interface Measure {
  /** Request + file version the measurement is valid for. */
  key: string
  result: SizeResult
  offset: { x: number; y: number } | null
}

interface MeasuresState {
  byTab: Record<string, Record<string, Measure>>
  running: Record<string, { done: number; total: number } | undefined>
  put: (tabId: string, fileId: string, measure: Measure) => void
  setRunning: (tabId: string, progress: { done: number; total: number } | undefined) => void
  forget: (tabId: string) => void
}

export const useTrimMeasures = create<MeasuresState>()((set) => ({
  byTab: {},
  running: {},
  put: (tabId, fileId, measure) =>
    set((s) => ({ byTab: { ...s.byTab, [tabId]: { ...s.byTab[tabId], [fileId]: measure } } })),
  setRunning: (tabId, progress) => set((s) => ({ running: { ...s.running, [tabId]: progress } })),
  forget: (tabId) =>
    set((s) => {
      const byTab = { ...s.byTab }
      delete byTab[tabId]
      return { byTab }
    }),
}))

export function measureKey(file: Pick<ImportedFile, 'path' | 'mtimeMs'>, request: OpRequest): string {
  return JSON.stringify({ p: file.path, m: file.mtimeMs, r: request.params })
}

/** Store a settled preview (success) as the file's measurement. */
export function recordMeasure(tabId: string, file: ImportedFile, request: OpRequest, width: number, height: number, meta: unknown): void {
  const offset = isTrimMeta(meta) ? { x: meta.trimRect.x, y: meta.trimRect.y } : null
  useTrimMeasures.getState().put(tabId, file.id, { key: measureKey(file, request), result: { ok: true, width, height }, offset })
}

/** Measurement of `file` valid for `request`, or null. */
export function lookupMeasure(tabId: string, file: ImportedFile, request: OpRequest | null): Measure | null {
  if (!request) return null
  const m = useTrimMeasures.getState().byTab[tabId]?.[file.id]
  return m && m.key === measureKey(file, request) ? m : null
}

/**
 * Measures every file without a valid measurement (sequentially, to keep memory low).
 * Stops early when `current()` no longer returns the same request (params changed).
 */
export async function measureAll(tabId: string, files: ImportedFile[], request: OpRequest, current: () => OpRequest | null): Promise<void> {
  const store = useTrimMeasures.getState()
  if (store.running[tabId]) return
  const todo = files.filter((f) => !lookupMeasure(tabId, f, request))
  const sameRequest = () => JSON.stringify(current()?.params) === JSON.stringify(request.params)
  store.setRunning(tabId, { done: 0, total: todo.length })
  try {
    for (let i = 0; i < todo.length; i++) {
      if (!sameRequest()) break
      const file = todo[i]
      try {
        const r = await previewOp(tabId, file.path, request)
        recordMeasure(tabId, file, request, r.width, r.height, r.meta)
      } catch (err) {
        useTrimMeasures.getState().put(tabId, file.id, { key: measureKey(file, request), result: { ok: false, error: toAppError(err) }, offset: null })
      }
      useTrimMeasures.getState().setRunning(tabId, { done: i + 1, total: todo.length })
    }
  } finally {
    useTrimMeasures.getState().setRunning(tabId, undefined)
  }
}
