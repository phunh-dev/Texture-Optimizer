import { LanguagesIcon, MonitorIcon, MoonIcon, SunIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Label, Separator } from '@/components/ui/misc'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { supportedLanguages, type Language } from '@/i18n'
import { useSettings, type Theme } from '@/stores/settings'
import { useUi } from '@/stores/ui'

/** Body of the settings dialog (exported for tests). */
export function SettingsPanel() {
  const { t } = useTranslation('settings')
  const language = useSettings((s) => s.language)
  const theme = useSettings((s) => s.theme)
  const recursive = useSettings((s) => s.recursiveImport)
  const themeItems = [
    { value: 'light' as const, label: t('theme.light'), Icon: SunIcon },
    { value: 'dark' as const, label: t('theme.dark'), Icon: MoonIcon },
    { value: 'system' as const, label: t('theme.system'), Icon: MonitorIcon },
  ]
  return (
    <div className="grid gap-5">
      <div className="grid gap-2">
        <Label className="flex items-center gap-1.5">
          <LanguagesIcon className="size-4 text-muted-foreground" />
          {t('language.label')}
        </Label>
        <ToggleGroup
          type="single"
          className="flex w-full"
          aria-label={t('language.label')}
          value={language}
          onValueChange={(v) => {
            if (supportedLanguages.includes(v as Language)) void useSettings.getState().setLanguage(v as Language)
          }}
        >
          {supportedLanguages.map((lang) => (
            <ToggleGroupItem key={lang} value={lang} className="h-8 text-[13px]">
              {t(`language.${lang}`)}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      <div className="grid gap-2">
        <Label>{t('theme.label')}</Label>
        <ToggleGroup
          type="single"
          className="flex w-full"
          aria-label={t('theme.label')}
          value={theme}
          onValueChange={(v) => {
            if (v) useSettings.getState().setTheme(v as Theme)
          }}
        >
          {themeItems.map(({ value, label, Icon }) => (
            <ToggleGroupItem key={value} value={value} className="h-8 text-[13px]">
              <Icon />
              {label}
            </ToggleGroupItem>
          ))}
        </ToggleGroup>
      </div>
      <Separator />
      <div className="flex items-start justify-between gap-4">
        <div className="space-y-1">
          <Label htmlFor="settings-recursive">{t('import.recursive')}</Label>
          <p className="text-xs text-muted-foreground">{t('import.recursiveDesc')}</p>
        </div>
        <Switch id="settings-recursive" checked={recursive} onCheckedChange={(v) => useSettings.getState().setRecursiveImport(v)} />
      </div>
    </div>
  )
}

export function SettingsDialog() {
  const { t } = useTranslation('settings')
  const open = useUi((s) => s.settingsOpen)
  const setOpen = useUi((s) => s.setSettingsOpen)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('title')}</DialogTitle>
          <DialogDescription>{t('description')}</DialogDescription>
        </DialogHeader>
        <SettingsPanel />
      </DialogContent>
    </Dialog>
  )
}
