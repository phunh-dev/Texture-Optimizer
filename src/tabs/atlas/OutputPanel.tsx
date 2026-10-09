import { FolderOpenIcon, LayersIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label, Spinner } from '@/components/ui/misc'
import { Switch } from '@/components/ui/switch'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { useLooseT } from '@/i18n/loose'
import { pickOutputFolder } from '@/lib/import'
import { cn } from '@/lib/utils'
import { requireSession, useSession, type Params } from '@/stores/session'

import { useExistingAtlas } from './hooks'
import { countStatuses } from './ipc'
import { baseNameProblem, INCREMENTAL_MODES, resolveParams, type IncrementalMode } from './schema'
import { StatusBadge } from './StatusBadge'

/** Atlas output: folder, base name, incremental behaviour and the existing atlas summary. */
export function AtlasOutputPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation('atlas')
  const lt = useLooseT()
  const params = useSession(tabId, (s) => s.params)
  const files = useSession(tabId, (s) => s.files)
  const p = resolveParams(params)
  const problem = baseNameProblem(p.baseName)
  const existing = useExistingAtlas(tabId, files, params)
  const id = (k: string) => `atlas-${tabId}-${k}`

  const set = (patch: Params, coalesce = false) => requireSession(tabId).getState().setParams(patch, { coalesce })
  const commit = () => requireSession(tabId).getState().commitParams()

  const chooseFolder = async () => {
    const path = await pickOutputFolder()
    if (path) set({ outputDir: path })
  }

  const counts = existing.summary ? countStatuses(existing.summary.plan) : null
  const modeDesc = p.incrementalMode === 'repackOptimal' ? t('output.mode.repackOptimalDesc') : t('output.mode.keepPositionsDesc')

  return (
    <section className="space-y-3" data-testid="atlas-output">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t('output.title')}</h2>
      <div className="space-y-4 rounded-lg border border-border bg-card/60 p-3">
        <div className="space-y-1.5">
          <Label htmlFor={id('folder')}>{t('output.folder')}</Label>
          <div className="flex items-center gap-1.5">
            <Input
              id={id('folder')}
              readOnly
              value={p.outputDir}
              placeholder={t('output.noFolder')}
              title={p.outputDir || undefined}
              className="flex-1 truncate font-mono text-xs"
            />
            <Button variant="outline" size="sm" onClick={() => void chooseFolder()} aria-label={t('output.chooseFolder')}>
              <FolderOpenIcon />
            </Button>
          </div>
        </div>

        <div className="space-y-1.5">
          <Label htmlFor={id('base')}>{t('output.baseName')}</Label>
          <Input
            id={id('base')}
            value={p.baseName}
            aria-invalid={!!problem}
            aria-describedby={id('base-desc')}
            className="font-mono"
            onChange={(e) => set({ baseName: e.target.value }, true)}
            onBlur={commit}
          />
          {problem ? (
            <p role="alert" className="text-xs font-medium text-destructive">
              {lt(`atlas:errors.baseName.${problem}`)}
            </p>
          ) : (
            <p id={id('base-desc')} className="text-xs text-muted-foreground">
              {t('output.baseNameDesc', { files: `${p.baseName}.png, ${p.baseName}.texatlas.json` })}
            </p>
          )}
        </div>

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

        {existing.path ? (
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
