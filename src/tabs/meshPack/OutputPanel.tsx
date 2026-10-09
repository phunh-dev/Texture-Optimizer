import { FolderOpenIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { Label } from '@/components/ui/misc'
import { pickOutputFolder } from '@/lib/import'
import { requireSession, useSession, type Params } from '@/stores/session'
import { isJobActive, useTabJob } from '@/stores/jobs'

import { isModelMeta, isSummaryMeta, type ModelOutcome } from './ipc'
import { atlasFileName, isValidBaseName, resolveParams } from './schema'

/** Output folder + base name of the atlas files. */
export function MeshOutputPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation('mesh')
  const params = useSession(tabId, (s) => s.params)
  const p = resolveParams(params)
  const id = (k: string) => `mesh-${tabId}-${k}`
  const set = (patch: Params, coalesce = false) => requireSession(tabId).getState().setParams(patch, { coalesce })
  const valid = isValidBaseName(p.baseName)

  const chooseFolder = async () => {
    const path = await pickOutputFolder()
    if (path) set({ outputDir: path })
  }

  return (
    <section className="space-y-3" data-testid="mesh-output">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t('output.title')}</h2>
      <div className="space-y-4 rounded-lg border border-border bg-card/60 p-3">
        <div className="space-y-1.5">
          <Label htmlFor={id('folder')}>{t('output.folder')}</Label>
          <div className="flex items-center gap-1.5">
            <Input
              id={id('folder')}
              value={p.outputDir}
              placeholder={t('output.noFolder')}
              title={p.outputDir || undefined}
              className="flex-1 truncate font-mono text-xs"
              onChange={(e) => set({ outputDir: e.target.value }, true)}
              onBlur={() => requireSession(tabId).getState().commitParams()}
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
            aria-invalid={!valid}
            className="font-mono"
            onChange={(e) => set({ baseName: e.target.value }, true)}
            onBlur={() => requireSession(tabId).getState().commitParams()}
          />
          {valid ? (
            <p className="text-xs text-muted-foreground">
              {t('output.baseNameDesc', { example: `${atlasFileName(p.baseName, 'baseColor')}, ${atlasFileName(p.baseName, 'normal')}, ${p.baseName}.report.json` })}
            </p>
          ) : (
            <p role="alert" className="text-xs font-medium text-destructive">
              {t('output.baseNameInvalid')}
            </p>
          )}
        </div>
      </div>
    </section>
  )
}

const OUTCOMES: ModelOutcome[] = ['rewritten', 'fallback', 'remapData', 'skipped', 'failed']

/** Outcome of the last finished pack job of the tab. */
export function LastRunPanel({ tabId }: { tabId: string }) {
  const { t } = useTranslation('mesh')
  const job = useTabJob(tabId)
  if (!job || isJobActive(job) || job.status !== 'done' || job.results.length === 0) return null
  const counts = Object.fromEntries(OUTCOMES.map((o) => [o, 0])) as Record<ModelOutcome, number>
  for (const r of job.results) if (isModelMeta(r.meta)) counts[r.meta.outcome]++
  const summary = job.results.map((r) => r.meta).find(isSummaryMeta)
  return (
    <section className="space-y-2" data-testid="mesh-last-run">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{t('lastRun.title')}</h2>
      <div className="flex flex-wrap gap-1.5 text-[11px]">
        {OUTCOMES.filter((o) => counts[o] > 0).map((o) => (
          <span key={o} className="rounded-full bg-muted px-2 py-0.5" data-outcome={o}>
            {t(`lastRun.${o}`)}: <b>{counts[o]}</b>
          </span>
        ))}
      </div>
      {summary?.files ? <p className="text-xs text-muted-foreground">{t('lastRun.files', { count: summary.files.length })}</p> : null}
    </section>
  )
}
