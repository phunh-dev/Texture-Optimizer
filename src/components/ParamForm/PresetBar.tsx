import { BookmarkIcon, BookmarkPlusIcon, ChevronDownIcon, RotateCcwIcon, Trash2Icon } from 'lucide-react'
import { useEffect, useState, type FormEvent } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { Button } from '@/components/ui/button'
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Tooltip } from '@/components/ui/tooltip'
import { usePresets, type Preset } from '@/stores/presets'
import { requireSession } from '@/stores/session'

export interface PresetBarProps {
  tabId: string
  /** Presets are shared by every tab of the same tool. */
  toolId: string
}

const EMPTY: Preset[] = []

export function PresetBar({ tabId, toolId }: PresetBarProps) {
  const { t } = useTranslation('common')
  const presets = usePresets((s) => s.byTool[toolId] ?? EMPTY)
  const [listOpen, setListOpen] = useState(false)
  const [saveOpen, setSaveOpen] = useState(false)
  const [name, setName] = useState('')

  useEffect(() => {
    void usePresets.getState().load(toolId)
  }, [toolId])

  const apply = (preset: Preset) => {
    requireSession(tabId).getState().applyPreset(preset.params)
    setListOpen(false)
    toast.success(t('presets.applied', { name: preset.name }))
  }

  const remove = async (preset: Preset) => {
    await usePresets.getState().remove(toolId, preset.name)
    toast(t('presets.deleted', { name: preset.name }))
  }

  const save = async (e: FormEvent) => {
    e.preventDefault()
    const trimmed = name.trim()
    if (!trimmed) return
    await usePresets.getState().save(toolId, trimmed, requireSession(tabId).getState().params)
    setSaveOpen(false)
    setName('')
    toast.success(t('presets.saved', { name: trimmed }))
  }

  const exists = presets.some((p) => p.name === name.trim())

  return (
    <div className="flex items-center gap-1" data-testid="preset-bar">
      <Popover open={listOpen} onOpenChange={setListOpen}>
        <PopoverTrigger asChild>
          <Button variant="outline" size="xs" className="min-w-0 flex-1 justify-between">
            <span className="flex min-w-0 items-center gap-1.5">
              <BookmarkIcon className="size-3.5" />
              <span className="truncate">{t('presets.label')}</span>
            </span>
            <ChevronDownIcon className="size-3.5 opacity-60" />
          </Button>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-64 p-1">
          {presets.length === 0 ? (
            <p className="px-3 py-4 text-center text-xs text-muted-foreground">{t('presets.empty')}</p>
          ) : (
            <ul className="max-h-64 overflow-y-auto" aria-label={t('presets.label')}>
              {presets.map((preset) => (
                <li key={preset.name} className="group/preset flex items-center gap-1 rounded-md hover:bg-accent">
                  <button
                    type="button"
                    onClick={() => apply(preset)}
                    className="min-w-0 flex-1 truncate rounded-md px-2.5 py-1.5 text-left text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                  >
                    {preset.name}
                  </button>
                  <button
                    type="button"
                    aria-label={t('presets.delete', { name: preset.name })}
                    onClick={() => void remove(preset)}
                    className="mr-1 rounded p-1 text-muted-foreground opacity-0 outline-none transition-opacity hover:text-destructive focus-visible:opacity-100 group-hover/preset:opacity-100"
                  >
                    <Trash2Icon className="size-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </PopoverContent>
      </Popover>

      <Tooltip content={t('presets.saveAs')}>
        <Button variant="ghost" size="icon-xs" aria-label={t('presets.saveAs')} onClick={() => setSaveOpen(true)}>
          <BookmarkPlusIcon />
        </Button>
      </Tooltip>
      <Tooltip content={t('actions.reset')}>
        <Button variant="ghost" size="icon-xs" aria-label={t('actions.reset')} onClick={() => requireSession(tabId).getState().resetParams()}>
          <RotateCcwIcon />
        </Button>
      </Tooltip>

      <Dialog open={saveOpen} onOpenChange={setSaveOpen}>
        <DialogContent className="max-w-sm">
          <form onSubmit={save} className="grid gap-4">
            <DialogHeader>
              <DialogTitle>{t('presets.saveTitle')}</DialogTitle>
              <DialogDescription>{t('presets.saveDescription')}</DialogDescription>
            </DialogHeader>
            <div className="grid gap-1.5">
              <label htmlFor={`preset-name-${tabId}`} className="text-[13px] font-medium">
                {t('presets.name')}
              </label>
              <Input
                id={`preset-name-${tabId}`}
                autoFocus
                value={name}
                maxLength={64}
                placeholder={t('presets.namePlaceholder')}
                onChange={(e) => setName(e.target.value)}
              />
              {exists ? <p className="text-xs text-amber-600 dark:text-amber-400">{t('presets.replaceHint')}</p> : null}
            </div>
            <DialogFooter>
              <Button variant="outline" onClick={() => setSaveOpen(false)}>
                {t('actions.cancel')}
              </Button>
              <Button type="submit" disabled={!name.trim()}>
                {t('actions.save')}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>
    </div>
  )
}
