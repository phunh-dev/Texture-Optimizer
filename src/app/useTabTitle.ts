import { useTranslation } from 'react-i18next'

import { getTool } from '@/tabs/registry'
import type { TabInfo } from '@/stores/tabs'

/** "Resize", "Resize 2", ... */
export function useTabTitle(): (tab: TabInfo) => string {
  const { t } = useTranslation('tabs')
  return (tab) => {
    const title = t(getTool(tab.toolId).titleKey)
    return tab.ordinal > 1 ? t('bar.tabTitle', { title, n: tab.ordinal }) : title
  }
}
