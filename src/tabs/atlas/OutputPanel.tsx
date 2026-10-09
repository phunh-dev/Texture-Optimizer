import { FolderOpenIcon, InfoIcon, LayersIcon, XIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label, Spinner } from '@/components/ui/misc'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { useLooseT } from '@/i18n/loose'
import { cn } from '@/lib/utils'
import { requireSession, useSession, type Params } from '@/stores/session'

import { clearTargetAtlas, pickTargetAtlas } from './export'
import { useExistingAtlas } from './hooks'
import { countStatuses } from './ipc'
import { EXPORTERS, INCREMENTAL_MODES, resolveParams, targetPagePath, writesMetadata, type ExporterKind, type IncrementalMode } from './schema'
import { StatusBadge } from './StatusBadge'

/**
 * Atlas output: the optional target atlas (an existing atlas to update; also
 * where Export… starts), incremental behaviour and the target's summary. The
 * image-only exporter never merges, so it shows a note instead.
 */
export function AtlasOutputPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation('atlas')
  const lt = useLooseT()
  const params = useSession(tabId, (s) => s.params)
  const files = useSession(tabId, (s) => s.files)
  const p = resolveParams(params)
  const exporter: ExporterKind = EXPORTERS.includes(p.exporter) ? p.exporter : 'genericJson'
  const incremental = writesMetadata(exporter)
  const target = targetPagePath(params)
  const existing = useExistingAtlas(tabId, files, params)
  const id = (k: string) => `atlas-${tabId}-${k}`

  const set = (patch: Params) => requireSession(tabId).getState().setParams(patch)

  const counts = existing.summary ? countStatuses(existing.summary.plan) : null
  const modeDesc = p.incrementalMode === 'repackOptimal' ? t('output.mode.repackOptimalDesc') : t('output.mode.keepPositionsDesc')

  return (
    <section className="space-y-3" data-testid="atlas-output">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t('output.title')}</h2>
      <div className="space-y-4 rounded-lg border border-border bg-card/60 p-3">
        <div className="space-y-1.5">
          <Label htmlFor={id('target')}>{t('target.label')}</Label>
          <div className="flex items-center gap-1.5">
            <Input
              id={id('target')}
              readOnly
              value={target}
              placeholder={t('target.none')}
              title={target || undefined}
              aria-describedby={id('target-desc')}
              className="flex-1 truncate font-mono text-xs"
            />
            <Button variant="outline" size="sm" onClick={() => void pickTargetAtlas(tabId)} aria-label={t('target.choose')} title={t('target.choose')}>
              <FolderOpenIcon />
            </Button>
            <Button
              variant="ghost"
              size="sm"
              disabled={!target}
              onClick={() => clearTargetAtlas(tabId)}
              aria-label={t('target.clear')}
              title={t('target.clear')}
            >
              <XIcon />
            </Button>
          </div>
          <p id={id('target-desc')} className="text-xs text-muted-foreground">
            {t('target.desc')}
          </p>
        </div>

        {incremental ? (
          <>
            <div className="space-y-1.5">
              <Label>{t('output.mode.label')}</Label>
              <ToggleGroup
                type="single"
                className="flex w-full"
                aria-label={t('output.mode.label')}
                value={p.incrementalMode}
                onValueChange={(v) => {
                  if (v) set({ incrementalMode: v as IncrementalMode })
                }}
              >
                {INCREMENTAL_MODES.map((m) => (
                  <ToggleGroupItem key={m} value={m}>
                    {t(`output.mode.${m}`)}
                  </ToggleGroupItem>
                ))}
              </ToggleGroup>
              <p className="text-xs text-muted-foreground">{modeDesc}</p>
            </div>

            <div className="space-y-1.5">
              <div className="flex items-center justify-between gap-3">
                <label htmlFor={id('remove')} className="text-[13px] font-medium leading-tight text-foreground/90">
                  {t('output.removeMissing.label')}
                </label>
                <Switch id={id('remove')} checked={p.removeMissing} onCheckedChange={(v) => set({ removeMissing: v })} />
              </div>
              <p className="text-xs text-muted-foreground">{t('output.removeMissing.desc')}</p>
            </div>
          </>
        ) : (
          <div className="flex gap-2 rounded-md bg-muted/60 p-2 text-xs text-muted-foreground" data-testid="atlas-image-only-note">
            <InfoIcon className="mt-0.5 size-3.5 shrink-0" />
            <div className="space-y-1">
              <p>{t('output.imageOnly.files', { base: p.baseName })}</p>
              <p>{t('output.imageOnly.noIncremental')}</p>
            </div>
          </div>
        )}

        {incremental && existing.path ? (
          <div className={cn('space-y-1.5 rounded-md border border-dashed border-border p-2 text-xs')} data-testid="atlas-existing">
            <div className="flex items-center gap-1.5 font-medium">
              <LayersIcon className="size-3.5 text-muted-foreground" />
              {t('output.existing.title')}
              {existing.loading ? <Spinner className="size-3 text-muted-foreground" /> : null}
            </div>
            {existing.summary ? (
              <>
                <p className="text-muted-foreground">
                  {t('output.existing.found', { count: existing.summary.frameCount, pages: existing.summary.pages.length })}
                </p>
                {existing.summary.exporter ? (
                  <p className="text-muted-foreground">
                    {t('output.existing.exporter', { exporter: lt(`atlas:exporter.kinds.${existing.summary.exporter}`) })}
                  </p>
                ) : null}
                {counts ? (
                  <div className="flex flex-wrap gap-1">
                    {(['kept', 'replaced', 'new', 'removed'] as const).map((s) =>
                      counts[s] > 0 ? <StatusBadge key={s} status={s} count={counts[s]} /> : null,
                    )}
                  </div>
                ) : null}
              </>
            ) : (
              <p className="text-muted-foreground">{existing.loading ? t('output.existing.checking') : t('output.existing.none')}</p>
            )}
          </div>
        ) : null}
      </div>
    </section>
  )
}
