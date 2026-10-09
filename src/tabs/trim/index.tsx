import { RulerIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ToolLayout, type ToolContext } from '@/components/ToolLayout'
import { Button } from '@/components/ui/button'
import { runOp } from '@/lib/ipc'
import type { ImportedFile, OpRequest, PreviewResult } from '@/lib/ipc/types'
import { requireSession, useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { OpPreview } from '../_imageOps/OpPreview'
import { SizeSummary } from '../_imageOps/SizeSummary'
import { trimFields } from './fields'
import { lookupMeasure, measureAll, recordMeasure, useTrimMeasures } from './measures'
import { buildTrimRequest, trimOutputSettings, trimPlacement } from './request'
import { isTrimMeta, trimSchema } from './schema'

const placement = (_file: ImportedFile, result: PreviewResult) => (isTrimMeta(result.meta) ? trimPlacement(result.meta) : undefined)

/** Run with the session output settings, plus the offsets JSON sidecar when enabled. */
export function runTrim(ctx: ToolContext): Promise<string> {
  const request = buildTrimRequest(ctx.params)
  if (!request) return Promise.reject({ code: 'INVALID_PARAMS', params: { param: 'params', reason: 'invalid' } })
  return runOp(
    ctx.tabId,
    request,
    ctx.files.map((f) => f.path),
    trimOutputSettings(ctx.output, ctx.params),
  )
}

function OffsetDetails({ result }: { result: PreviewResult }) {
  const { t } = useTranslation('trim')
  if (!isTrimMeta(result.meta)) return null
  return (
    <span className="shrink-0 tabular-nums" data-testid="trim-offset">
      {t('preview.offset', { x: result.meta.trimRect.x, y: result.meta.trimRect.y })}
    </span>
  )
}

function MeasureFooter({ tabId, request }: { tabId: string; request: OpRequest | null }) {
  const { t } = useTranslation('trim')
  const files = useSession(tabId, (s) => s.files)
  const running = useTrimMeasures((s) => s.running[tabId])
  useTrimMeasures((s) => s.byTab[tabId]) // re-render when measurements arrive
  const missing = request ? files.filter((f) => !lookupMeasure(tabId, f, request)).length : 0
  if (files.length === 0) return null
  return (
    <div className="space-y-1.5">
      <p className="text-xs text-muted-foreground">{t('sizes.hint')}</p>
      <Button
        variant="outline"
        size="sm"
        className="w-full"
        disabled={!request || missing === 0 || !!running}
        data-testid="measure-sizes"
        onClick={() => {
          if (!request) return
          void measureAll(tabId, files, request, () => buildTrimRequest(requireSession(tabId).getState().params))
        }}
      >
        <RulerIcon />
        {running ? t('sizes.measuring', { done: running.done, total: running.total }) : t('sizes.measure', { count: missing })}
      </Button>
    </div>
  )
}

export default function TrimTab({ tabId }: ToolTabProps) {
  const params = useSession(tabId, (s) => s.params)
  useTrimMeasures((s) => s.byTab[tabId]) // re-render the summary when measurements change
  const request = buildTrimRequest(params)
  const predict = (file: ImportedFile) => lookupMeasure(tabId, file, request)?.result ?? null

  return (
    <ToolLayout
      tabId={tabId}
      fields={trimFields}
      schema={trimSchema}
      buildRequest={(ctx) => buildTrimRequest(ctx.params)}
      run={runTrim}
      preview={(ctx) => (
        <OpPreview
          ctx={ctx}
          request={buildTrimRequest(ctx.params)}
          placement={placement}
          details={(_file, result) => <OffsetDetails result={result} />}
          onResult={(file, result, req) => recordMeasure(tabId, file, req, result.width, result.height, result.meta)}
        />
      )}
      sidePanelBottom={<SizeSummary tabId={tabId} predict={predict} footer={<MeasureFooter tabId={tabId} request={request} />} />}
    />
  )
}
