import { TriangleAlertIcon } from 'lucide-react'
import { useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { Spinner } from '@/components/ui/misc'
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { useElementSize } from '@/hooks/useElementSize'
import { useLooseT } from '@/i18n/loose'
import { translateError } from '@/lib/errors'
import type { AppError, ImportedFile } from '@/lib/ipc/types'
import { cn } from '@/lib/utils'
import { requireSession, type Params } from '@/stores/session'

import type { MaterialReport, ModelReport, PreviewPayload } from './ipc'
import { UvStatusBadge } from './ModelList'
import { usePackPreview } from './preview'

const normPath = (p: string) => p.replace(/\\/g, '/').toLowerCase()

/** Session file of a report model (by path). */
export function fileForModel(model: ModelReport, files: ImportedFile[]): ImportedFile | null {
  const want = normPath(model.source)
  return files.find((f) => normPath(f.path) === want) ?? null
}

export interface RectOverlay {
  model: ModelReport
  material: MaterialReport
  file: ImportedFile | null
}

/** Rect overlays of one page (materials sharing a block are listed once per material). */
export function overlaysForPage(data: PreviewPayload, page: number, files: ImportedFile[]): RectOverlay[] {
  const out: RectOverlay[] = []
  for (const model of data.report.models) {
    const file = fileForModel(model, files)
    for (const material of model.materials) {
      if (material.page === page && material.rect) out.push({ model, material, file })
    }
  }
  return out
}

interface PageViewProps {
  data: PreviewPayload
  url: string
  page: number
  files: ImportedFile[]
  selectedIds: string[]
  tabId: string
}

function PageView({ data, url, page, files, selectedIds, tabId }: PageViewProps) {
  const { t } = useTranslation('mesh')
  const ref = useRef<HTMLDivElement>(null)
  const size = useElementSize(ref)
  const info = data.report.pages[page]
  const overlays = useMemo(() => overlaysForPage(data, page, files), [data, page, files])
  const [hover, setHover] = useState<RectOverlay | null>(null)
  const pad = 16
  const scale = info ? Math.max(0.01, Math.min((size.width - pad * 2) / info.width, (size.height - pad * 2) / info.height)) : 1
  const w = info ? info.width * scale : 0
  const h = info ? info.height * scale : 0

  return (
    <div ref={ref} className="relative min-h-0 flex-1 overflow-hidden rounded-lg border border-border bg-card" data-testid="atlas-page">
      {info ? (
        <div className="bg-checker absolute" style={{ width: w, height: h, left: (size.width - w) / 2, top: (size.height - h) / 2 }}>
          <img src={url} alt={t('preview.page', { n: page + 1 })} className="size-full" style={{ imageRendering: scale > 1 ? 'pixelated' : 'auto' }} draggable={false} />
          {overlays.map((o) => {
            const r = o.material.rect!
            const selected = o.file ? selectedIds.includes(o.file.id) : false
            const title = t('preview.rectTitle', { model: o.model.name, material: o.material.name, width: r.width, height: r.height })
            return (
              <button
                key={`${o.model.source}#${o.material.materialIndex}`}
                type="button"
                title={title}
                aria-label={title}
                data-testid="atlas-rect"
                data-selected={selected}
                onMouseEnter={() => setHover(o)}
                onMouseLeave={() => setHover((h2) => (h2 === o ? null : h2))}
                onFocus={() => setHover(o)}
                onClick={() => {
                  if (o.file) requireSession(tabId).getState().select(o.file.id)
                }}
                className={cn(
                  'absolute border outline-none transition-colors',
                  selected ? 'border-2 border-primary bg-primary/20' : 'border-white/70 hover:border-primary hover:bg-primary/15 focus-visible:border-primary',
                )}
                style={{
                  left: `${(r.x / info.width) * 100}%`,
                  top: `${(r.y / info.height) * 100}%`,
                  width: `${(r.width / info.width) * 100}%`,
                  height: `${(r.height / info.height) * 100}%`,
                }}
              />
            )
          })}
        </div>
      ) : null}
      {hover?.material.rect ? (
        <div className="pointer-events-none absolute left-2 top-2 rounded-md bg-black/70 px-2 py-1 text-[11px] text-white backdrop-blur" data-testid="rect-hover">
          {t('preview.rectTitle', {
            model: hover.model.name,
            material: hover.material.name,
            width: hover.material.rect.width,
            height: hover.material.rect.height,
          })}
        </div>
      ) : null}
    </div>
  )
}

function Warnings({ warnings }: { warnings: AppError[] }) {
  const { t } = useTranslation('mesh')
  if (warnings.length === 0) return null
  const unique = [...new Map(warnings.map((w) => [JSON.stringify(w), w])).values()]
  return (
    <section className="space-y-1" data-testid="preview-warnings">
      <h3 className="flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
        <TriangleAlertIcon className="size-3" />
        {t('preview.warnings')}
      </h3>
      <ul className="space-y-0.5 text-[11px] text-amber-700 dark:text-amber-300">
        {unique.slice(0, 8).map((w, i) => (
          <li key={i}>{translateError(w)}</li>
        ))}
      </ul>
    </section>
  )
}

export interface MeshAtlasPreviewProps {
  tabId: string
  files: ImportedFile[]
  params: Params
  selectedIds: string[]
  /** Injected in tests. */
  state?: ReturnType<typeof usePackPreview>
}

export function MeshAtlasPreview({ tabId, files, params, selectedIds, state }: MeshAtlasPreviewProps) {
  const live = usePackPreview(tabId, state ? [] : files, params)
  const { data, urls, error, loading } = state ?? live
  const { t } = useTranslation('mesh')
  const lt = useLooseT()
  const [pageSel, setPage] = useState(0)

  if (files.length === 0) {
    return <div className="flex size-full items-center justify-center text-sm text-muted-foreground">{t('preview.empty')}</div>
  }
  if (!data) {
    return (
      <div className="flex size-full flex-col items-center justify-center gap-2 text-sm text-muted-foreground" data-testid="preview-status">
        {error ? (
          <>
            <span className="font-medium text-destructive">{t('preview.failed')}</span>
            <span className="max-w-md text-center text-xs">{translateError(error)}</span>
          </>
        ) : (
          <>
            <Spinner className="size-5" />
            {t('preview.loading')}
          </>
        )}
      </div>
    )
  }

  const page = Math.min(pageSel, data.report.pages.length - 1)
  const info = data.report.pages[page]
  const allWarnings = [...data.report.warnings, ...data.report.models.flatMap((m) => m.warnings)]
  const channelName = data.channel.startsWith('other:') ? data.channel : lt(`mesh:channels.${data.channel}`)

  return (
    <div className="flex size-full min-h-0 gap-3" data-testid="mesh-preview">
      <div className="flex min-w-0 flex-1 flex-col gap-2">
        <div className="flex flex-wrap items-center gap-2 text-xs">
          {data.report.pages.length > 1 ? (
            <ToggleGroup type="single" value={String(page)} onValueChange={(v) => v && setPage(Number(v))} aria-label={t('preview.pages', { count: data.report.pages.length })}>
              {data.report.pages.map((p) => (
                <ToggleGroupItem key={p.index} value={String(p.index)}>
                  {t('preview.page', { n: p.index + 1 })}
                </ToggleGroupItem>
              ))}
            </ToggleGroup>
          ) : null}
          <span className="rounded-full bg-primary/10 px-2 py-0.5 font-medium text-primary" data-testid="page-size">
            {t('preview.pageSize', { width: info.width, height: info.height })}
          </span>
          <span className="text-muted-foreground" data-testid="occupancy">
            {t('preview.occupancy', { value: Math.round(info.occupancy * 100) })}
          </span>
          <span className="text-muted-foreground">{t('preview.scale', { value: Math.round(data.report.scalePercent * 10) / 10 })}</span>
          <span className="text-muted-foreground">{t('preview.channel', { channel: channelName })}</span>
          {loading ? <Spinner className="size-3.5 text-muted-foreground" /> : null}
          {error ? <span className="text-destructive">{translateError(error)}</span> : null}
        </div>
        <PageView data={data} url={urls[page] ?? ''} page={page} files={files} selectedIds={selectedIds} tabId={tabId} />
      </div>
      <aside className="w-64 shrink-0 space-y-3 overflow-y-auto" data-testid="preview-materials">
        <h3 className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">{t('preview.materials')}</h3>
        {data.report.models.map((m) => {
          const file = fileForModel(m, files)
          const selected = file ? selectedIds.includes(file.id) : false
          return (
            <section key={m.source} className={cn('space-y-1 rounded-md border p-2', selected ? 'border-primary' : 'border-border')}>
              <h4 className="truncate text-xs font-semibold" title={m.source}>
                {m.name}
              </h4>
              {m.error ? <p className="text-[11px] text-destructive">{translateError(m.error)}</p> : null}
              <ul className="space-y-1">
                {m.materials.map((mat) => (
                  <li key={mat.materialIndex} className="flex items-center gap-1.5 text-[11px]" data-testid="preview-material">
                    <span className="min-w-0 flex-1 truncate">{mat.name || '—'}</span>
                    {mat.page != null && data.report.pages.length > 1 ? <span className="text-muted-foreground">{t('preview.page', { n: mat.page + 1 })}</span> : null}
                    <UvStatusBadge status={mat.status} tiles={mat.tiles} />
                  </li>
                ))}
              </ul>
            </section>
          )
        })}
        <Warnings warnings={allWarnings} />
      </aside>
    </div>
  )
}
