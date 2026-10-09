// Smart Atlas tab: packing params + one exporter, live atlas preview, own
// output section (folder, base name, incremental mode) and an export job.
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { ToolLayout } from '@/components/ToolLayout'
import { useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { AtlasPreview } from './AtlasPreview'
import { startExport } from './export'
import { atlasFields } from './fields'
import { AtlasOutputPanel } from './OutputPanel'
import { atlasSchema, EXPORTERS, type ExporterKind } from './schema'

export default function AtlasTab({ tabId }: ToolTabProps) {
  const { t } = useTranslation('atlas')
  const rawExporter = useSession(tabId, (s) => s.params.exporter)
  const outputDir = useSession(tabId, (s) => (typeof s.params.outputDir === 'string' ? s.params.outputDir.trim() : ''))
  const exporter: ExporterKind = EXPORTERS.includes(rawExporter as ExporterKind) ? (rawExporter as ExporterKind) : 'genericJson'
  const fields = useMemo(() => atlasFields(exporter), [exporter])

  return (
    <ToolLayout
      tabId={tabId}
      fields={fields}
      schema={atlasSchema}
      run={startExport}
      preview={(ctx) => <AtlasPreview tabId={ctx.tabId} files={ctx.files} params={ctx.params} selectedIds={ctx.selectedIds} />}
      sidePanelBottom={<AtlasOutputPanel tabId={tabId} />}
      showOutput={false}
      runLabel={t('run.export')}
      runDisabledReason={outputDir ? null : t('run.noFolder')}
    />
  )
}
