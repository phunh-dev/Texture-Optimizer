// Export flow. Nothing is written until the user confirms a native Save As
// dialog for the main page `<base>.png`: its folder + file name give the
// export folder and base name, then the `atlas_export` job starts. After a
// successful export the saved atlas becomes the tab's target atlas (default
// location of the next export, incremental update). Reports the outcome with
// an atlas-specific toast (instead of the generic "Processed N images").
import { open, save } from '@tauri-apps/plugin-dialog'
import { toast } from 'sonner'

import i18n from '@/i18n'
import { tKey } from '@/i18n/loose'
import { translateError } from '@/lib/errors'
import type { JobFinishedEvent } from '@/lib/ipc/types'
import { setJobFinishHandler, useJobs } from '@/stores/jobs'
import { requireSession } from '@/stores/session'

import { useAtlasRefresh } from './hooks'
import { atlasExport, atlasLoadProject, isExportSummary } from './ipc'
import { baseNameProblem, buildAtlasRequest, joinPath, projectFilePath, resolveParams, splitPath, targetPagePath } from './schema'
import { translateAtlasWarning } from './warnings'

const PROJECT_SUFFIX = '.texatlas.json'

/** Base name problem as a translated message, or null when valid. */
function baseNameError(base: string): string | null {
  const problem = baseNameProblem(base)
  return problem ? tKey(`atlas:errors.baseName.${problem}`) : null
}

/** Folder + base name of a chosen `<base>.png` (extension optional). */
export function targetFromPagePath(path: string): { dir: string; base: string } {
  const { dir, file } = splitPath(path)
  return { dir, base: file.replace(/\.png$/i, '') }
}

/** Where the Save As dialog starts: the target atlas, else `<first image folder>/<base>.png`. */
export function defaultSavePath(tabId: string): string | undefined {
  const s = requireSession(tabId).getState()
  const target = targetPagePath(s.params)
  if (target) return target
  const p = resolveParams(s.params)
  const base = baseNameProblem(p.baseName) ? 'atlas' : p.baseName
  const first = s.files[0]?.path
  if (!first) return `${base}.png`
  const { dir } = splitPath(first)
  return dir ? joinPath(dir, `${base}.png`) : `${base}.png`
}

export function reportExport(tabId: string, e: JobFinishedEvent, saved?: { dir: string; base: string }): void {
  useAtlasRefresh.getState().bump(tabId)
  const t = i18n.t
  if (e.cancelled) {
    toast.info(t('common:jobs.cancelled'))
    return
  }
  const failed = e.results.find((r) => r.error)
  if (failed) {
    toast.error(t('atlas:toast.failed'), { description: translateAtlasWarning(failed.error) })
    return
  }
  // The saved atlas becomes the target: next Export… defaults there and updates it.
  if (saved) requireSession(tabId).getState().setParams({ outputDir: saved.dir, baseName: saved.base })
  const summary = e.results.map((r) => r.meta).find(isExportSummary)
  if (!summary) {
    toast.success(t('common:jobs.finished', { count: e.results.length }))
    return
  }
  const lines = [t('atlas:toast.details', { count: summary.written.length })]
  if (summary.deleted.length > 0) lines.push(t('atlas:toast.deleted', { count: summary.deleted.length }))
  lines.push(...summary.warnings.slice(0, 3).map((w) => translateAtlasWarning(w)))
  toast.success(t('atlas:toast.exported', { dir: summary.outputDir }), { description: lines.join('\n') })
}

/**
 * Export…: asks where to save (native Save As), then starts the export job.
 * Cancel, an invalid file name or a dialog error write nothing. Resolves with
 * the job id, or null when nothing was started.
 */
export async function exportWithDialog(tabId: string): Promise<string | null> {
  const t = i18n.t
  let chosen: string | null
  try {
    chosen = await save({
      title: t('atlas:export.dialogTitle'),
      defaultPath: defaultSavePath(tabId),
      filters: [{ name: t('atlas:export.pngFilter'), extensions: ['png'] }],
    })
  } catch (err) {
    toast.error(translateError(err))
    return null
  }
  if (!chosen) return null
  const saved = targetFromPagePath(chosen)
  const problem = baseNameError(saved.base)
  if (problem || !saved.dir) {
    toast.error(problem ?? tKey('atlas:errors.baseName.empty'))
    return null
  }

  const s = requireSession(tabId).getState()
  const request = buildAtlasRequest({ ...s.params, outputDir: saved.dir, baseName: saved.base })
  const paths = s.files.map((f) => f.path)
  return useJobs.getState().start(tabId, paths.length, async () => {
    setJobFinishHandler(tabId, (e) => reportExport(tabId, e, saved))
    try {
      return await atlasExport(tabId, paths, request)
    } catch (err) {
      setJobFinishHandler(tabId, null)
      throw err
    }
  })
}

/**
 * Target atlas of a picked file: `<base>.texatlas.json`, a page `<base>.png`
 * or `<base>_<i>.png` of a multi-page atlas (when `<base>` has a project and
 * `<base>_<i>` has none), or any other file named after the atlas.
 */
export async function targetFromPickedFile(path: string, files: string[], removeMissing: boolean): Promise<{ dir: string; base: string }> {
  const { dir, file } = splitPath(path)
  if (file.toLowerCase().endsWith(PROJECT_SUFFIX)) return { dir, base: file.slice(0, -PROJECT_SUFFIX.length) }
  const stem = file.replace(/\.[^.]+$/, '')
  const page = /^(.+)_\d+$/.exec(stem)
  if (page && dir) {
    const has = (base: string) =>
      atlasLoadProject(projectFilePath(dir, base), files, removeMissing).then(
        (s) => s !== null,
        () => false,
      )
    if (!(await has(stem)) && (await has(page[1]))) return { dir, base: page[1] }
  }
  return { dir, base: stem }
}

/** Opens a file picker for an existing atlas (page PNG or project file) and makes it the target. */
export async function pickTargetAtlas(tabId: string): Promise<void> {
  const t = i18n.t
  const s = requireSession(tabId).getState()
  try {
    const selected = await open({
      title: t('atlas:target.dialogTitle'),
      multiple: false,
      directory: false,
      defaultPath: targetPagePath(s.params) || undefined,
      filters: [{ name: t('atlas:target.filter'), extensions: ['png', 'json'] }],
    })
    if (typeof selected !== 'string' || !selected) return
    const p = resolveParams(s.params)
    const target = await targetFromPickedFile(
      selected,
      s.files.map((f) => f.path),
      p.removeMissing,
    )
    const problem = baseNameError(target.base)
    if (problem || !target.dir) {
      toast.error(problem ?? tKey('atlas:errors.baseName.empty'))
      return
    }
    requireSession(tabId).getState().setParams({ outputDir: target.dir, baseName: target.base })
  } catch (err) {
    toast.error(translateError(err))
  }
}

/** Forgets the target atlas (the next export starts a new atlas). */
export function clearTargetAtlas(tabId: string): void {
  requireSession(tabId).getState().setParams({ outputDir: '' })
}
