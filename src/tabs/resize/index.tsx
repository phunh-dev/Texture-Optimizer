import { useMemo } from 'react'

import { ToolLayout } from '@/components/ToolLayout'
import type { ImportedFile } from '@/lib/ipc/types'
import { useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { OpPreview } from '../_imageOps/OpPreview'
import { SizeSummary } from '../_imageOps/SizeSummary'
import { resizeFields } from './fields'
import { buildResizeRequest, predictResize } from './request'
import { resizeSchema } from './schema'

/** The result covers the whole original (it is a rescaled copy). */
const placement = (file: ImportedFile) => ({ x: 0, y: 0, width: file.width, height: file.height })

export default function ResizeTab({ tabId }: ToolTabProps) {
  const params = useSession(tabId, (s) => s.params)
  const parsed = useMemo(() => resizeSchema.safeParse(params), [params])
  const predict = (file: ImportedFile) => (parsed.success ? predictResize(file.width, file.height, parsed.data) : null)

  return (
    <ToolLayout
      tabId={tabId}
      fields={resizeFields}
      schema={resizeSchema}
      buildRequest={(ctx) => buildResizeRequest(ctx.params)}
      preview={(ctx) => <OpPreview ctx={ctx} request={buildResizeRequest(ctx.params)} placement={placement} />}
      sidePanelBottom={<SizeSummary tabId={tabId} predict={predict} />}
    />
  )
}
