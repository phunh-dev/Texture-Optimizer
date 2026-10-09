// Export flow: start the `atlas_export` job and report its outcome with an
// atlas-specific toast (instead of the generic "Processed N images").
import { toast } from 'sonner'

import type { ToolContext } from '@/components/ToolLayout'
import i18n from '@/i18n'
import type { JobFinishedEvent } from '@/lib/ipc/types'
import { setJobFinishHandler } from '@/stores/jobs'

import { useAtlasRefresh } from './hooks'
import { atlasExport, isExportSummary } from './ipc'
import { buildAtlasRequest } from './schema'
import { translateAtlasWarning } from './warnings'

export function reportExport(tabId: string, e: JobFinishedEvent): void {
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

/** ToolLayout `run`: starts the export job and resolves with its id. */
export async function startExport(ctx: ToolContext): Promise<string> {
  const request = buildAtlasRequest(ctx.params)
  setJobFinishHandler(ctx.tabId, (e) => reportExport(ctx.tabId, e))
  try {
    return await atlasExport(
      ctx.tabId,
      ctx.files.map((f) => f.path),
      request,
    )
  } catch (err) {
    setJobFinishHandler(ctx.tabId, null)
    throw err
  }
}
