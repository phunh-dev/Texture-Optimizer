// Background Remover tab: white / checker / picked color / auto removal with
// a live before/after preview, detection info and an eyedropper.
import { GridIcon, SparklesIcon, SquareIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ToolLayout } from '@/components/ToolLayout'
import { Button } from '@/components/ui/button'
import { requireSession } from '@/stores/session'
import type { ToolTabProps } from '@/tabs/registry'

import { bgRemoveFields, PICK_FLAG } from './fields'
import { BgRemovePreview } from './Preview'
import { bgRemoveSchema, buildBgRemoveRequest, quickPresetParams, type QuickPresetId } from './schema'

const QUICK: { id: QuickPresetId; icon: typeof SquareIcon }[] = [
  { id: 'white', icon: SquareIcon },
  { id: 'checker', icon: GridIcon },
  { id: 'auto', icon: SparklesIcon },
]

function QuickPresets({ tabId }: { tabId: string }) {
  const { t } = useTranslation('bgremove')
  const apply = (id: QuickPresetId) => {
    const s = requireSession(tabId).getState()
    s.applyPreset(quickPresetParams(id))
    s.setUiFlag(PICK_FLAG, false)
  }
  return (
    <section className="space-y-2">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t('presets.title')}</h2>
      <div className="grid grid-cols-3 gap-1.5">
        {QUICK.map(({ id, icon: Icon }) => (
          <Button key={id} variant="outline" size="sm" className="h-auto flex-col gap-1 py-2" onClick={() => apply(id)}>
            <Icon />
            <span className="text-xs">{t(`presets.${id}`)}</span>
          </Button>
        ))}
      </div>
    </section>
  )
}

export default function BgRemoveTab({ tabId }: ToolTabProps) {
  return (
    <ToolLayout
      tabId={tabId}
      fields={bgRemoveFields}
      schema={bgRemoveSchema}
      buildRequest={(ctx) => buildBgRemoveRequest(ctx.params)}
      preview={(ctx) => <BgRemovePreview tabId={ctx.tabId} file={ctx.focusFile} />}
      sidePanelTop={<QuickPresets tabId={tabId} />}
    />
  )
}
