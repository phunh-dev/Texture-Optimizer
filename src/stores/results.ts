// Staged results of the image tools. A run only processes into the tab's
// staging folder; this store keeps what came out so the tab can show it
// (Original / Result), save it where the user chooses, or discard it.
// It is session UI state: never part of undo/redo, never persisted.
import { open, save } from '@tauri-apps/plugin-dialog'
import { toast } from 'sonner'
import { create } from 'zustand'

import i18n from '@/i18n'
import { translateError } from '@/lib/errors'
import { discardResults, listResults, saveResults } from '@/lib/ipc'
import type { AppError, ImportedFile, JobFinishedEvent, SaveReport, SaveTarget, StagedResult } from '@/lib/ipc/types'

import { outputWarnings, setJobFinishHandler, summarizeJob } from './jobs'
import { getSession, onSessionDeleted, type SessionData } from './session'

export type ResultsView = 'original' | 'result'

export interface ResultItem {
  /** Source image the result was made from. */
  source: string
  /** Header info of the staged result (its `path` is inside the staging folder). */
  file: ImportedFile
  /** Staged metadata sidecar, if any. */
  sidecar: string | null
  /** Non-fatal output warnings (e.g. JPEG kept as PNG to keep transparency). */
  warnings: AppError[]
}

export interface TabResults {
  jobId: string
  /** Successfully processed files, in source order. */
  items: ResultItem[]
  failed: number
  cancelled: boolean
  /** `resultSignature` of the inputs at run time (to detect outdated results). */
  signature: string
  view: ResultsView
}

interface ResultsState {
  byTab: Record<string, TabResults>
  /** Tabs whose Save is in progress. */
  saving: Record<string, boolean>
}

export const useResults = create<ResultsState>()(() => ({ byTab: {}, saving: {} }))

/** What a run depends on: files (and their versions), params and output settings. */
export function resultSignature(s: Pick<SessionData, 'files' | 'params' | 'output'>): string {
  return JSON.stringify({ files: s.files.map((f) => [f.path, f.mtimeMs]), params: s.params, output: s.output })
}

// Bumped whenever a tab's results are replaced or dropped, so a late
// `listResults` answer of an older run is ignored.
const generation = new Map<string, number>()
const bump = (tabId: string) => {
  const next = (generation.get(tabId) ?? 0) + 1
  generation.set(tabId, next)
  return next
}

function patchTab(tabId: string, results: TabResults | null): void {
  useResults.setState((s) => {
    const byTab = { ...s.byTab }
    if (results) byTab[tabId] = results
    else delete byTab[tabId]
    return { byTab }
  })
}

const baseName = (path: string) => path.split(/[\\/]/).pop() ?? path

/** Folder of `path`, keeping its own separator style. */
export function dirName(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  return i > 0 ? path.slice(0, i) : i === 0 ? path.slice(0, 1) : ''
}

/** `dir` + `name` with the separator `dir` already uses. */
export function joinPath(dir: string, name: string): string {
  if (!dir) return name
  const sep = dir.includes('\\') && !dir.includes('/') ? '\\' : '/'
  return dir.endsWith('/') || dir.endsWith('\\') ? `${dir}${name}` : `${dir}${sep}${name}`
}

/** Placeholder info when the staged file could not be listed (thumbnail still works by path). */
function fallbackFile(path: string): ImportedFile {
  const name = baseName(path)
  return { id: path, path, name, ext: name.split('.').pop()?.toLowerCase() ?? '', width: 0, height: 0, sizeBytes: 0, mtimeMs: Date.now() }
}

/**
 * Prepare a staged run of `tabId`: drops the previous results (the backend
 * replaces them too) and collects the results when the job finishes.
 */
export function beginStagedRun(tabId: string, signature: string): void {
  const gen = bump(tabId)
  patchTab(tabId, null)
  setJobFinishHandler(tabId, (e) => {
    summarizeJob(e, { hint: i18n.t('common:results.notSavedToast') })
    void receiveResults(tabId, signature, e, gen)
  })
}

async function receiveResults(tabId: string, signature: string, e: JobFinishedEvent, gen: number): Promise<void> {
  const ok = e.results.filter((r) => r.output && !r.error)
  const failed = e.results.filter((r) => r.error && r.error.code !== 'CANCELLED').length
  if (ok.length === 0) return
  let staged: StagedResult[] = []
  try {
    staged = await listResults(tabId, e.jobId)
  } catch (err) {
    console.warn('[results] cannot list staged results', err)
  }
  if (generation.get(tabId) !== gen || !getSession(tabId)) return
  const byName = new Map(staged.map((s) => [s.file.name, s]))
  const items: ResultItem[] = ok.map((r) => {
    const output = r.output as string
    const listed = byName.get(baseName(output))
    return {
      source: r.input,
      file: listed?.file ?? fallbackFile(output),
      sidecar: listed?.sidecar ?? null,
      warnings: outputWarnings(r.meta),
    }
  })
  patchTab(tabId, { jobId: e.jobId, items, failed, cancelled: e.cancelled, signature, view: 'result' })
}

export function setResultsView(tabId: string, view: ResultsView): void {
  const current = useResults.getState().byTab[tabId]
  if (!current || current.view === view) return
  patchTab(tabId, { ...current, view })
}

/** Drop the tab's results from the UI only (the backend deletes the files itself). */
export function forgetResults(tabId: string): void {
  bump(tabId)
  setJobFinishHandler(tabId, null)
  patchTab(tabId, null)
}

/** Discard button: deletes the staged files and the results state. */
export async function discardTabResults(tabId: string): Promise<void> {
  forgetResults(tabId)
  try {
    await discardResults(tabId)
  } catch (err) {
    toast.error(translateError(err))
  }
}

function notifySaved(report: SaveReport): void {
  const t = i18n.t
  const count = report.saved.length
  const folder = report.destination
  if (report.failed.length > 0) {
    const details = report.failed
      .slice(0, 3)
      .map((f) => `${baseName(f.path)}: ${translateError(f.error)}`)
      .join('\n')
    toast.error(t('common:results.saveFailed', { count: report.failed.length, saved: count }), { description: details })
    return
  }
  if (report.skipped.length > 0) {
    toast.warning(t('common:results.savedWithSkipped', { count, folder, skipped: report.skipped.length }))
    return
  }
  toast.success(t('common:results.saved', { count, folder }))
}

/**
 * Save…: one result → native Save As (default: source folder + staged name),
 * several → folder picker (default: folder of the first source). Cancelling
 * the dialog does nothing. Can be repeated to save to other places.
 */
export async function saveTabResults(tabId: string): Promise<void> {
  const results = useResults.getState().byTab[tabId]
  const session = getSession(tabId)?.getState()
  if (!results || results.items.length === 0 || !session) return
  let target: SaveTarget
  try {
    if (results.items.length === 1) {
      const item = results.items[0]
      const ext = item.file.name.includes('.') ? (item.file.name.split('.').pop() ?? '') : ''
      const chosen = await save({
        defaultPath: joinPath(dirName(item.source), item.file.name),
        filters: ext ? [{ name: i18n.t('common:results.imageFilter', { ext: ext.toUpperCase() }), extensions: [ext] }] : undefined,
      })
      if (!chosen) return
      target = { kind: 'file', path: chosen }
    } else {
      const chosen = await open({ directory: true, multiple: false, defaultPath: dirName(results.items[0].source) || undefined })
      if (!chosen || Array.isArray(chosen)) return
      target = { kind: 'folder', path: chosen }
    }
  } catch (err) {
    toast.error(translateError(err))
    return
  }
  useResults.setState((s) => ({ saving: { ...s.saving, [tabId]: true } }))
  try {
    const report = await saveResults(tabId, results.jobId, target, session.output.conflict, session.output)
    notifySaved(report)
  } catch (err) {
    toast.error(translateError(err))
  } finally {
    useResults.setState((s) => {
      const saving = { ...s.saving }
      delete saving[tabId]
      return { saving }
    })
  }
}

// A closed tab's results go away with it (the session deletes the files).
onSessionDeleted((tabId) => forgetResults(tabId))

export function useTabResults(tabId: string): TabResults | undefined {
  return useResults((s) => s.byTab[tabId])
}
