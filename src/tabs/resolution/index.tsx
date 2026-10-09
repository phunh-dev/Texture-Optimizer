import { useMemo } from 'react'

import { ToolLayout } from '@/components/ToolLayout'
import type { ImportedFile, OpRequest, PreviewResult } from '@/lib/ipc/types'
import { useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { OpPreview } from '../_imageOps/OpPreview'
import { SizeSummary } from '../_imageOps/SizeSummary'
import { resolutionFields } from './fields'
import { buildResolutionRequest, predictResolution, resolutionPlacement } from './request'
import { resolutionSchema } from './schema'

const placement = (file: ImportedFile, result: PreviewResult, request: OpRequest) =>
  resolutionPlacement(file.width, file.height, result.width, result.height, request)

export default function ResolutionTab({ tabId }: ToolTabProps) {
  const params = useSession(tabId, (s) => s.params)
  const parsed = useMemo(() => resolutionSchema.safeParse(params), [params])
  const predict = (file: ImportedFile) => (parsed.success ? predictResolution(file.width, file.height, parsed.data) : null)

  return (
    <ToolLayout
      tabId={tabId}
      fields={resolutionFields}
      schema={resolutionSchema}
      buildRequest={(ctx) => buildResolutionRequest(ctx.params)}
      preview={(ctx) => <OpPreview ctx={ctx} request={buildResolutionRequest(ctx.params)} placement={placement} />}
      sidePanelBottom={<SizeSummary tabId={tabId} predict={predict} />}
    />
  )
}
