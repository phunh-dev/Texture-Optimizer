import { SparklesIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { MOD_KEY } from '@/components/ToolLayout/HistoryButtons'
import { Kbd } from '@/components/ui/misc'
import { useTabs } from '@/stores/tabs'

import { ToolCards } from './ToolCards'

/** Shown when no tab is open. */
export function HomeScreen() {
  const { t } = useTranslation(['tabs', 'common'])
  const shortcuts = [
    { keys: `${MOD_KEY}+T`, label: t('home.shortcutNewTab') },
    { keys: `${MOD_KEY}+W`, label: t('home.shortcutCloseTab') },
    { keys: 'Ctrl+Tab', label: t('home.shortcutNextTab') },
    { keys: `${MOD_KEY}+Z`, label: t('home.shortcutUndo') },
  ]
  return (
    <div className="size-full overflow-y-auto" data-testid="home-screen">
      <div className="mx-auto flex max-w-5xl flex-col gap-8 px-8 py-12">
        <div className="space-y-3">
          <span className="inline-flex items-center gap-1.5 rounded-full bg-primary/10 px-3 py-1 text-xs font-medium text-primary">
            <SparklesIcon className="size-3.5" />
            {t('common:app.title')}
          </span>
          <h1 className="text-3xl font-semibold tracking-tight">{t('home.title')}</h1>
          <p className="max-w-2xl text-muted-foreground">{t('home.subtitle')}</p>
        </div>
        <ToolCards onPick={(id) => useTabs.getState().openTab(id)} />
        <div className="flex flex-wrap items-center gap-x-5 gap-y-2 text-xs text-muted-foreground">
          <span className="font-medium text-foreground/70">{t('home.shortcutsTitle')}</span>
          {shortcuts.map((s) => (
            <span key={s.keys} className="flex items-center gap-1.5">
              <Kbd>{s.keys}</Kbd>
              {s.label}
            </span>
          ))}
        </div>
      </div>
    </div>
  )
}
