// Run flow: Pack… first asks for the output folder (native folder picker,
// default = last used folder or the first model's folder); only then the
// `mesh_pack` job starts. Its outcome is reported with a packer-specific
// toast ("rewritten N, fallback M, skipped K").
import { open } from '@tauri-apps/plugin-dialog'
import { toast } from 'sonner'

import i18n from '@/i18n'
import { translateError } from '@/lib/errors'
import type { JobFinishedEvent } from '@/lib/ipc/types'
import { setJobFinishHandler, useJobs } from '@/stores/jobs'
import { requireSession } from '@/stores/session'

import { isModelMeta, isSummaryMeta, meshPack } from './ipc'
import { buildPackOptions, resolveParams } from './schema'

export function reportPack(e: JobFinishedEvent): void {
  const t = i18n.t
  if (e.cancelled) {
    toast.info(t('common:jobs.cancelled'))
    return
  }
  const summaryEntry = e.results.find((r) => isSummaryMeta(r.meta))
  if (summaryEntry?.error) {
    toast.error(t('mesh:toast.failed'), { description: translateError(summaryEntry.error) })
    return
  }
  const models = e.results.filter((r) => isModelMeta(r.meta))
  const count = (o: string) => models.filter((r) => isModelMeta(r.meta) && r.meta.outcome === o).length
  const failed = models.filter((r) => r.error)
  const remap = count('remapData')
  const title =
    remap > 0 && count('rewritten') === 0
      ? t('mesh:toast.doneRemap', { count: remap, skipped: count('skipped') })
      : t('mesh:toast.done', { rewritten: count('rewritten'), fallback: count('fallback'), skipped: count('skipped') })
  const lines: string[] = []
  if (failed.length > 0) {
    lines.push(t('mesh:toast.failedModels', { count: failed.length }))
    lines.push(...failed.slice(0, 3).map((r) => `${r.input.split(/[\\/]/).pop()}: ${translateError(r.error)}`))
  }
  const warnings = [
    ...((isSummaryMeta(summaryEntry?.meta) ? summaryEntry.meta.warnings : undefined) ?? []),
    ...models.flatMap((r) => (isModelMeta(r.meta) ? r.meta.warnings : [])),
  ]
  lines.push(...warnings.slice(0, 2).map((w) => translateError(w)))
  const description = lines.length > 0 ? lines.join('\n') : undefined
  if (failed.length > 0) toast.warning(title, { description })
  else toast.success(title, { description })
}

/** Folder of a file path ('' when it has none). */
function folderOf(path: string): string {
  const i = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'))
  if (i < 0) return ''
  return i === 0 ? path.slice(0, 1) : path.slice(0, i)
}

/** Where the folder picker starts: the last used folder, else the first model's folder. */
export function defaultPackFolder(tabId: string): string | undefined {
  const s = requireSession(tabId).getState()
  const last = resolveParams(s.params).outputDir.trim()
  if (last) return last
  const first = s.files[0]?.path
  return (first && folderOf(first)) || undefined
}

/**
 * Pack…: asks for the output folder, then starts the pack job there. Cancel
 * or a dialog error start nothing. Resolves with the job id, or null.
 */
export async function packWithDialog(tabId: string): Promise<string | null> {
  let chosen: string | string[] | null
  try {
    chosen = await open({
      title: i18n.t('mesh:run.dialogTitle'),
      directory: true,
      multiple: false,
      defaultPath: defaultPackFolder(tabId),
    })
  } catch (err) {
    toast.error(translateError(err))
    return null
  }
  const dir = Array.isArray(chosen) ? chosen[0] : chosen
  if (!dir) return null
  const session = requireSession(tabId).getState()
  // Remembered as the next default folder.
  session.setParams({ outputDir: dir })
  const p = resolveParams(session.params)
  const models = session.files.map((f) => f.path)
  const options = buildPackOptions(session.params)
  return useJobs.getState().start(tabId, models.length, async () => {
    setJobFinishHandler(tabId, reportPack)
    try {
      return await meshPack(tabId, models, options, dir, p.baseName)
    } catch (err) {
      setJobFinishHandler(tabId, null)
      throw err
    }
  })
}
