// Run flow: start the `mesh_pack` job and report its outcome with a
// packer-specific toast ("rewritten N, fallback M, skipped K").
import { toast } from 'sonner'

import type { ToolContext } from '@/components/ToolLayout'
import i18n from '@/i18n'
import { translateError } from '@/lib/errors'
import type { JobFinishedEvent } from '@/lib/ipc/types'
import { setJobFinishHandler } from '@/stores/jobs'

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

/** ToolLayout `run`: starts the pack job and resolves with its id. */
export async function startPack(ctx: ToolContext): Promise<string> {
  const p = resolveParams(ctx.params)
  setJobFinishHandler(ctx.tabId, reportPack)
  try {
    return await meshPack(
      ctx.tabId,
      ctx.files.map((f) => f.path),
      buildPackOptions(ctx.params),
      p.outputDir.trim(),
      p.baseName,
    )
  } catch (err) {
    setJobFinishHandler(ctx.tabId, null)
    throw err
  }
}
