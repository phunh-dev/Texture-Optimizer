import { useTranslation } from 'react-i18next'

import { Label } from '@/components/ui/misc'
import { NumberInput } from '@/components/ui/number-input'
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select'
import { Slider } from '@/components/ui/slider'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import type { ConflictPolicy, OutputFormat, OutputSettings } from '@/lib/ipc/types'
import { requireSession, useSession } from '@/stores/session'

const FORMATS: OutputFormat[] = ['keep', 'png', 'tga', 'jpg', 'webp']
const COMPRESSIONS: OutputSettings['pngCompression'][] = ['fast', 'default', 'best']
const CONFLICTS: ConflictPolicy[] = ['autoRename', 'overwrite', 'skip']

/**
 * Output format / encoding panel bound to the session's `output` (undoable).
 * There is no destination: runs are staged and the user picks where to save
 * the results afterwards; the conflict policy applies at that point.
 */
export function OutputSettingsPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation('common')
  const output = useSession(tabId, (s) => s.output)

  const set = (patch: Partial<OutputSettings>, coalesce = false) => requireSession(tabId).getState().setOutput(patch, { coalesce })
  const commit = () => requireSession(tabId).getState().commitParams()

  const showPng = output.format === 'png' || output.format === 'keep'
  // WebP output is lossless, so quality only applies to JPEG.
  const showQuality = output.format === 'jpg'
  const id = (k: string) => `out-${tabId}-${k}`

  return (
    <div className="space-y-4" data-testid="output-settings">
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
        <div className="space-y-1.5">
          <Label htmlFor={id('conflict')}>{t('output.conflict')}</Label>
          <Select value={output.conflict} onValueChange={(v) => set({ conflict: v as ConflictPolicy })}>
            <SelectTrigger id={id('conflict')} size="sm" aria-describedby={id('conflict-hint')}>
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
      </div>
      <p id={id('conflict-hint')} className="text-xs text-muted-foreground" data-testid="output-save-hint">
        {t('output.saveHint')}
      </p>

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
