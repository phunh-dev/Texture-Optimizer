// 3D Texture Packer tab: model cards instead of the image grid, packing
// params, live atlas layout preview and a pack job (worker process).
import { useEffect } from 'react'
import { useTranslation } from 'react-i18next'

import { ToolLayout } from '@/components/ToolLayout'
import { registerTabImporter } from '@/lib/import'
import { useSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { MeshAtlasPreview } from './AtlasPreview'
import { meshFields } from './fields'
import { ModelList } from './ModelList'
import { modelImporter } from './models'
import { LastRunPanel, MeshOutputPanel } from './OutputPanel'
import { startPack } from './run'
import { isValidBaseName, meshSchema } from './schema'

export default function MeshPackTab({ tabId }: ToolTabProps) {
  const { t } = useTranslation('mesh')
  const count = useSession(tabId, (s) => s.files.length)
  const outputDir = useSession(tabId, (s) => (typeof s.params.outputDir === 'string' ? s.params.outputDir.trim() : ''))
  const baseName = useSession(tabId, (s) => (typeof s.params.baseName === 'string' ? s.params.baseName : ''))

  useEffect(() => registerTabImporter(tabId, modelImporter), [tabId])

  const reason = !outputDir ? t('run.noFolder') : !isValidBaseName(baseName) ? t('run.badBaseName') : null

  return (
    <ToolLayout
      tabId={tabId}
      fields={meshFields}
      schema={meshSchema}
      run={startPack}
      content={(ctx) => <ModelList tabId={ctx.tabId} />}
      preview={(ctx) => <MeshAtlasPreview tabId={ctx.tabId} files={ctx.files} params={ctx.params} selectedIds={ctx.selectedIds} />}
      sidePanelBottom={
        <>
          <MeshOutputPanel tabId={tabId} />
          <LastRunPanel tabId={tabId} />
        </>
      }
      showOutput={false}
      countLabel={t('list.count', { count })}
      runLabel={t('run.label', { count })}
      runDisabledReason={reason}
    />
  )
}
