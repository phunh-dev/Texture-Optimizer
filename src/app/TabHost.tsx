import { createElement, lazy, Suspense, type ComponentType, type LazyExoticComponent } from 'react'
import { useTranslation } from 'react-i18next'

import { Spinner } from '@/components/ui/misc'
import { useActiveTab } from '@/stores/tabs'
import { tools, type ToolId, type ToolTabProps } from '@/tabs/registry'

// One lazy (code-split) component per tool, created once at module load.
const lazyTools: ReadonlyMap<ToolId, LazyExoticComponent<ComponentType<ToolTabProps>>> = new Map(
  tools.map((tool) => [tool.id, lazy(tool.load)]),
)

function TabFallback() {
  const { t } = useTranslation('common')
  return (
    <div className="flex size-full items-center justify-center gap-2 text-sm text-muted-foreground">
      <Spinner />
      {t('loading')}
    </div>
  )
}

/**
 * Mounts ONLY the active tab (sleep/awake): inactive tabs keep their state in
 * stores outside React and are unmounted, so their <img> elements are freed.
 */
export function TabHost() {
  const active = useActiveTab()
  const Component = active ? lazyTools.get(active.toolId) : undefined
  if (!active || !Component) return null
  return (
    <div role="tabpanel" className="size-full min-h-0" data-testid="tab-host">
      <Suspense fallback={<TabFallback />}>
        {/* Components are created once at module load (see lazyTools), not per render. */}
        {createElement(Component, { key: active.id, tabId: active.id })}
      </Suspense>
    </div>
  )
}
