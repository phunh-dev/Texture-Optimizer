// Model details (meshes, materials, textures) live outside the undo history:
// the session only tracks the model files (path, id), so remove → undo just
// works. Details are keyed by path and re-scanned when missing (e.g. after a
// restart restored the session).
import { useEffect } from 'react'
import { toast } from 'sonner'
import { create } from 'zustand'

import i18n from '@/i18n'
import { translateError } from '@/lib/errors'
import type { ImportSummary, TabImporter } from '@/lib/import'
import type { AppError, ImportedFile } from '@/lib/ipc/types'
import { requireSession } from '@/stores/session'

import { meshScan, type ModelInfo } from './ipc'

export const MODEL_EXTENSIONS = ['fbx', 'obj', 'dae'] as const

interface ModelInfoStore {
  infos: Record<string, ModelInfo>
  /** Paths whose scan failed (error shown on the card). */
  errors: Record<string, AppError>
  pending: Record<string, true>
  put: (infos: ModelInfo[]) => void
  fail: (path: string, error: AppError) => void
  setPending: (paths: string[], pending: boolean) => void
}

export const useModelInfos = create<ModelInfoStore>()((set) => ({
  infos: {},
  errors: {},
  pending: {},
  put: (list) =>
    set((s) => {
      const infos = { ...s.infos }
      const errors = { ...s.errors }
      for (const info of list) {
        infos[info.path] = info
        delete errors[info.path]
      }
      return { infos, errors }
    }),
  fail: (path, error) => set((s) => ({ errors: { ...s.errors, [path]: error } })),
  setPending: (paths, pending) =>
    set((s) => {
      const next = { ...s.pending }
      for (const p of paths) {
        if (pending) next[p] = true
        else delete next[p]
      }
      return { pending: next }
    }),
}))

/** Adds the models found under `paths` to the tab (one undo step) and toasts a summary. */
export async function importModels(tabId: string, paths: string[], recursive: boolean): Promise<ImportSummary | null> {
  if (paths.length === 0) return null
  const t = i18n.t
  try {
    const result = await meshScan(paths, recursive)
    useModelInfos.getState().put(result.models.map((m) => m.info))
    const added = requireSession(tabId)
      .getState()
      .addFiles(result.models.map((m) => m.file))
    const summary = { added, duplicates: result.models.length - added, skipped: result.skipped.length }
    if (summary.skipped > 0) {
      toast.warning(t('mesh:toast.importSkipped', { count: added, skipped: summary.skipped }), {
        description: translateError(result.skipped[0].error),
      })
    } else if (added > 0) toast.success(t('mesh:toast.importAdded', { count: added }))
    else if (summary.duplicates > 0) toast.info(t('mesh:toast.importDuplicates'))
    else toast.info(t('mesh:toast.importNothing'))
    return summary
  } catch (err) {
    toast.error(translateError(err))
    return null
  }
}

export const modelImporter: TabImporter = {
  importPaths: (tabId, paths, options) => importModels(tabId, paths, options.recursive),
  filterNameKey: 'mesh:toast.modelsFilter',
  extensions: MODEL_EXTENSIONS,
}

/** Scans models of the list whose details are unknown (restored sessions). */
export function useEnsureModelInfos(files: ImportedFile[]): void {
  useEffect(() => {
    const { infos, pending, errors } = useModelInfos.getState()
    const missing = files.map((f) => f.path).filter((p) => !infos[p] && !pending[p] && !errors[p])
    if (missing.length === 0) return
    const store = useModelInfos.getState()
    store.setPending(missing, true)
    meshScan(missing, false).then(
      (result) => {
        store.put(result.models.map((m) => m.info))
        for (const s of result.skipped) store.fail(s.path, s.error)
        const found = new Set(result.models.map((m) => m.info.path))
        for (const p of missing) {
          if (!found.has(p) && !result.skipped.some((s) => s.path === p)) store.fail(p, { code: 'MESH_IMPORT_FAILED', params: { path: p, detail: '' } })
        }
        store.setPending(missing, false)
      },
      (err: unknown) => {
        for (const p of missing) store.fail(p, { code: 'UNKNOWN', params: { detail: String(err) } })
        store.setPending(missing, false)
      },
    )
  }, [files])
}

/** Removes models from the tab with an "Undo" toast. */
export function removeModelsWithUndo(tabId: string, ids: string[]): number {
  const session = requireSession(tabId)
  const removed = session.getState().removeFiles(ids)
  if (removed > 0) {
    toast(i18n.t('mesh:list.removedToast', { count: removed }), {
      id: `remove-${tabId}`,
      action: { label: i18n.t('common:actions.undo'), onClick: () => requireSession(tabId).getState().undo() },
    })
  }
  return removed
}
