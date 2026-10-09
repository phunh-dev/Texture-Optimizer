// Live rename plan: re-computed by the backend (debounced) whenever files,
// params or the destination change, or after a rename / revert touched the disk.
import { createContext, useContext, useEffect, useMemo, useState } from 'react'
import { create } from 'zustand'

import { toAppError } from '@/lib/errors'
import type { AppError } from '@/lib/ipc/types'
import { useSession } from '@/stores/session'

import { renamePlan, type RenamePlanItem } from './api'
import { toExecuteMode, toRenameParams } from './schema'

export const PLAN_DEBOUNCE_MS = 300

export interface PlanState {
  items: RenamePlanItem[]
  error: AppError | null
  loading: boolean
}

export interface PlanSummary {
  total: number
  /** Items whose path changes (copy mode: every item). */
  changed: number
  unchanged: number
  conflicts: number
}

const EMPTY: PlanState = { items: [], error: null, loading: false }

/** Bumped after operations that changed the disk, so open plans re-validate. */
export const usePlanRefresh = create<{ tick: number; bump: () => void }>()((set) => ({
  tick: 0,
  bump: () => set((s) => ({ tick: s.tick + 1 })),
}))

export const RenamePlanContext = createContext<PlanState>(EMPTY)
export const useRenamePlanContext = () => useContext(RenamePlanContext)

export const fileName = (p: string) => p.split(/[\\/]/).pop() ?? p

export const isUnchanged = (item: RenamePlanItem, copy: boolean) => !copy && item.from === item.to


export function summarize(items: RenamePlanItem[], copy: boolean): PlanSummary {
  const conflicts = items.filter((i) => i.conflict).length
  const unchanged = items.filter((i) => isUnchanged(i, copy)).length
  return { total: items.length, changed: items.length - unchanged, unchanged, conflicts }
}

export function useRenamePlan(tabId: string): PlanState {
  const files = useSession(tabId, (s) => s.files)
  const params = useSession(tabId, (s) => s.params)
  const tick = usePlanRefresh((s) => s.tick)
  const key = useMemo(() => {
    const mode = toExecuteMode(params)
    // Without a folder yet, preview the names in place.
    const effective = mode.kind === 'copyTo' && !mode.dir ? { kind: 'inPlace' as const } : mode
    return JSON.stringify({ paths: files.map((f) => f.path), params: toRenameParams(params), mode: effective })
  }, [files, params])
  const [state, setState] = useState<PlanState & { key: string }>({ ...EMPTY, key: '' })

  useEffect(() => {
    const { paths, params: p, mode } = JSON.parse(key) as { paths: string[]; params: ReturnType<typeof toRenameParams>; mode: ReturnType<typeof toExecuteMode> }
    let cancelled = false
    const timer = setTimeout(
      () => {
        if (paths.length === 0) {
          setState({ ...EMPTY, key })
          return
        }
        setState((s) => ({ ...s, loading: true }))
        renamePlan(paths, p, mode)
          .then((items) => {
            if (!cancelled) setState({ items, error: null, loading: false, key })
          })
          .catch((err: unknown) => {
            if (!cancelled) setState({ items: [], error: toAppError(err), loading: false, key })
          })
      },
      paths.length === 0 ? 0 : PLAN_DEBOUNCE_MS,
    )
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [key, tick])

  // A plan computed for older inputs is "loading" until the new one arrives.
  return { items: state.items, error: state.error, loading: state.loading || state.key !== key }
}
