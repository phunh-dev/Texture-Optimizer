// Smart Atlas tab: packing params + one exporter, live atlas preview (in
// memory, writes nothing), own output section (optional target atlas,
// incremental mode) and Export…, which asks where to save before writing.
import { useMemo } from 'react'

import { ToolLayout } from '@/components/ToolLayout'
import { useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { AtlasPreview } from './AtlasPreview'
import { ExportRunPanel } from './ExportRunPanel'
import { atlasFields } from './fields'
import { AtlasOutputPanel } from './OutputPanel'
import { atlasSchema, EXPORTERS, type ExporterKind } from './schema'

export default function AtlasTab({ tabId }: ToolTabProps) {
  const rawExporter = useSession(tabId, (s) => s.params.exporter)
  const exporter: ExporterKind = EXPORTERS.includes(rawExporter as ExporterKind) ? (rawExporter as ExporterKind) : 'genericJson'
  const fields = useMemo(() => atlasFields(exporter), [exporter])

  return (
    <ToolLayout
      tabId={tabId}
      fields={fields}
      schema={atlasSchema}
      preview={(ctx) => <AtlasPreview tabId={ctx.tabId} files={ctx.files} params={ctx.params} selectedIds={ctx.selectedIds} />}
      sidePanelBottom={<AtlasOutputPanel tabId={tabId} />}
      showOutput={false}
      runPanel={<ExportRunPanel tabId={tabId} />}
    />
  )
}
