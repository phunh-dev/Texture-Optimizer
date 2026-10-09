import { FolderOpenIcon, TriangleAlertIcon } from 'lucide-react'
import { useRef } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/misc'
import { NumberInput } from '@/components/ui/number-input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { pickOutputFolder } from '@/lib/import'
import type { OutputFormat, OutputMode, OutputSettings } from '@/lib/ipc/types'
import { requireSession, useSession } from '@/stores/session'

const FORMATS: OutputFormat[] = ['keep', 'png', 'tga', 'jpg', 'webp']
const COMPRESSIONS: OutputSettings['pngCompression'][] = ['fast', 'default', 'best']
const CONFLICTS: OutputSettings['conflict'][] = ['autoRename', 'overwrite', 'skip']

/** Returns an i18n key explaining why the output settings cannot be used, or null. */
export function outputProblem(output: OutputSettings): 'output.noFolder' | null {
  return output.mode.kind === 'folder' && !output.mode.path ? 'output.noFolder' : null
}

/** Output destination / format panel bound to the session's `output` (undoable). */
export function OutputSettingsPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation('common')
  const output = useSession(tabId, (s) => s.output)
  // Remember the other modes' values when switching back and forth.
  const lastModes = useRef<{ folder: string; suffix: string }>({ folder: '', suffix: '_opt' })

  const set = (patch: Partial<OutputSettings>, coalesce = false) => requireSession(tabId).getState().setOutput(patch, { coalesce })
  const commit = () => requireSession(tabId).getState().commitParams()

  const setMode = (kind: OutputMode['kind']) => {
    const current = output.mode
    if (current.kind === 'folder') lastModes.current.folder = current.path
    if (current.kind === 'suffix') lastModes.current.suffix = current.suffix
    if (kind === 'inPlace') set({ mode: { kind } })
    if (kind === 'folder') set({ mode: { kind, path: lastModes.current.folder } })
    if (kind === 'suffix') set({ mode: { kind, suffix: lastModes.current.suffix || '_opt' } })
  }

  const chooseFolder = async () => {
    const path = await pickOutputFolder()
    if (path) set({ mode: { kind: 'folder', path } })
  }

  const showPng = output.format === 'png' || output.format === 'keep'
  // WebP output is lossless, so quality only applies to JPEG.
  const showQuality = output.format === 'jpg'
  const id = (k: string) => `out-${tabId}-${k}`

  return (
    <div className="space-y-4" data-testid="output-settings">
      <div className="space-y-1.5">
        <Label>{t('output.mode')}</Label>
        <ToggleGroup
          type="single"
          className="flex w-full"
          aria-label={t('output.mode')}
          value={output.mode.kind}
          onValueChange={(v) => {
            if (v) setMode(v as OutputMode['kind'])
          }}
        >
          <ToggleGroupItem value="suffix">{t('output.modeSuffix')}</ToggleGroupItem>
          <ToggleGroupItem value="folder">{t('output.modeFolder')}</ToggleGroupItem>
          <ToggleGroupItem value="inPlace">{t('output.modeInPlace')}</ToggleGroupItem>
        </ToggleGroup>

        {output.mode.kind === 'inPlace' && (
          <p className="flex items-center gap-1.5 text-xs text-amber-600 dark:text-amber-400">
            <TriangleAlertIcon className="size-3.5 shrink-0" />
            {t('output.inPlaceWarning')}
          </p>
        )}
        {output.mode.kind === 'folder' && (
          <div className="flex items-center gap-1.5">
            <div
              className="flex h-8 min-w-0 flex-1 items-center rounded-md border border-border bg-muted/50 px-2.5 text-xs"
              title={output.mode.path || undefined}
            >
              <span className={output.mode.path ? 'truncate [direction:rtl]' : 'truncate text-muted-foreground'}>
                {output.mode.path || t('output.noFolder')}
              </span>
            </div>
            <Button variant="outline" size="sm" onClick={() => void chooseFolder()} aria-label={t('output.chooseFolder')}>
              <FolderOpenIcon />
            </Button>
          </div>
        )}
        {output.mode.kind === 'suffix' && (
          <div className="space-y-1.5 pt-1">
            <Label htmlFor={id('suffix')}>{t('output.suffix')}</Label>
            <Input
              id={id('suffix')}
              className="h-8 font-mono text-[13px]"
              value={output.mode.suffix}
              spellCheck={false}
              onChange={(e) => set({ mode: { kind: 'suffix', suffix: e.target.value } }, true)}
              onBlur={commit}
            />
          </div>
        )}
      </div>

      <div className="grid grid-cols-2 gap-3">
        <div className="space-y-1.5">
          <Label htmlFor={id('format')}>{t('output.format')}</Label>
          <Select value={output.format} onValueChange={(v) => set({ format: v as OutputFormat })}>
            <SelectTrigger id={id('format')} size="sm">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {FORMATS.map((f) => (
                <SelectItem key={f} value={f}>
                  {t(`output.formats.${f}`)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        {output.mode.kind !== 'inPlace' && (
          <div className="space-y-1.5">
            <Label htmlFor={id('conflict')}>{t('output.conflict')}</Label>
            <Select value={output.conflict} onValueChange={(v) => set({ conflict: v as OutputSettings['conflict'] })}>
              <SelectTrigger id={id('conflict')} size="sm">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                {CONFLICTS.map((c) => (
                  <SelectItem key={c} value={c}>
                    {t(`output.conflicts.${c}`)}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>
        )}
      </div>

      {showPng && (
        <div className="space-y-1.5">
          <Label>{t('output.pngCompression')}</Label>
          <ToggleGroup
            type="single"
            className="flex w-full"
            aria-label={t('output.pngCompression')}
            value={output.pngCompression}
            onValueChange={(v) => {
              if (v) set({ pngCompression: v as OutputSettings['pngCompression'] })
            }}
          >
            {COMPRESSIONS.map((c) => (
              <ToggleGroupItem key={c} value={c}>
                {t(`output.compression.${c}`)}
              </ToggleGroupItem>
            ))}
          </ToggleGroup>
        </div>
      )}

      {showQuality && (
        <div className="space-y-1.5">
          <Label htmlFor={id('quality')}>{t('output.jpgQuality')}</Label>
          <div className="flex items-center gap-3">
            <Slider
              aria-label={t('output.jpgQuality')}
              min={1}
              max={100}
              value={[output.jpgQuality]}
              onValueChange={([v]) => set({ jpgQuality: v }, true)}
              onValueCommit={commit}
              className="flex-1"
            />
            <NumberInput
              id={id('quality')}
              className="w-20 shrink-0"
              value={output.jpgQuality}
              min={1}
              max={100}
              hideSteppers
              onValueChange={(v) => set({ jpgQuality: Math.round(Math.min(100, Math.max(1, v))) }, true)}
              onCommit={commit}
            />
          </div>
        </div>
      )}

      {showPng && (
        <div className="flex items-start justify-between gap-3">
          <div className="space-y-1">
            <Label htmlFor={id('oxipng')}>{t('output.optimizePng')}</Label>
            <p className="text-xs text-muted-foreground">{t('output.optimizePngDesc')}</p>
          </div>
          <Switch id={id('oxipng')} checked={output.optimizePng} onCheckedChange={(v) => set({ optimizePng: v })} />
        </div>
      )}
    </div>
  )
}
