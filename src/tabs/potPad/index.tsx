import { useMemo } from 'react'

import { ToolLayout } from '@/components/ToolLayout'
import type { ImportedFile, OpRequest, PreviewResult } from '@/lib/ipc/types'
import { useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { OpPreview } from '../_imageOps/OpPreview'
import { SizeSummary } from '../_imageOps/SizeSummary'
import { potPadFields } from './fields'
import { buildPotPadRequest, potPadPlacement, predictPotPad } from './request'
import { potPadSchema } from './schema'

const placement = (file: ImportedFile, result: PreviewResult, request: OpRequest) =>
  potPadPlacement(file.width, file.height, result.width, result.height, request)

export default function PotPadTab({ tabId }: ToolTabProps) {
  const params = useSession(tabId, (s) => s.params)
  const parsed = useMemo(() => potPadSchema.safeParse(params), [params])
  const predict = (file: ImportedFile) => (parsed.success ? predictPotPad(file.width, file.height, parsed.data) : null)

  return (
    <ToolLayout
      tabId={tabId}
      fields={potPadFields}
      schema={potPadSchema}
      buildRequest={(ctx) => buildPotPadRequest(ctx.params)}
      preview={(ctx) => <OpPreview ctx={ctx} request={buildPotPadRequest(ctx.params)} placement={placement} />}
      sidePanelBottom={<SizeSummary tabId={tabId} predict={predict} />}
    />
  )
}
