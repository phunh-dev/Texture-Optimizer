import { Maximize2Icon, ScanIcon, TriangleAlertIcon, ZoomInIcon, ZoomOutIcon } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/misc'
import { Tooltip } from '@/components/ui/tooltip'
import type { ImportedFile } from '@/lib/ipc/types'
import { clamp, cn } from '@/lib/utils'
import { requireSession, type Params } from '@/stores/session'

import { useAtlasPreview, type PreviewData } from './hooks'
import { countStatuses, type AtlasFrame, type SpriteStatus } from './ipc'
import { STATUS_STYLES, StatusBadge } from './StatusBadge'
import { translateAtlasWarning } from './warnings'

const MIN_ZOOM = 0.05
const MAX_ZOOM = 32
const DRAG_THRESHOLD = 3

interface View {
  zoom: number
  x: number
  y: number
}

const normPath = (p: string) => p.replace(/\\/g, '/').toLowerCase()

/** The grid file a frame came from (by source path, else by file stem). */
export function fileForFrame(frame: AtlasFrame, files: ImportedFile[]): ImportedFile | null {
  if (frame.sourcePath) {
    const want = normPath(frame.sourcePath)
    const hit = files.find((f) => normPath(f.path) === want)
    if (hit) return hit
  }
  const names = [frame.name, ...frame.aliases]
  return files.find((f) => names.includes(f.name.replace(/\.[^.]+$/, ''))) ?? null
}

export interface AtlasPreviewProps {
  tabId: string
  files: ImportedFile[]
  params: Params
  selectedIds: string[]
}

/** Live atlas preview: rebuilt (debounced) whenever files or params change. */
export function AtlasPreview({ tabId, files, params, selectedIds }: AtlasPreviewProps) {
  const { t } = useTranslation('atlas')
  const { data, error, loading } = useAtlasPreview(tabId, files, params)

  if (files.length === 0) {
    return <CenterNote>{t('preview.empty')}</CenterNote>
  }
  if (!data) {
    return error ? (
      <ErrorNote error={error} />
    ) : (
      <CenterNote>
        <Spinner className="text-primary" />
        {t('preview.loading')}
      </CenterNote>
    )
  }
  return (
    <AtlasView
      data={data}
      files={files}
      selectedIds={selectedIds}
      loading={loading}
      error={error}
      onSelectFile={(id) => requireSession(tabId).getState().select(id)}
    />
  )
}

function CenterNote({ children }: { children: React.ReactNode }) {
  return (
    <div className="flex size-full items-center justify-center gap-2 rounded-lg border border-dashed border-border text-sm text-muted-foreground" data-testid="atlas-preview-note">
      {children}
    </div>
  )
}

function ErrorNote({ error, compact }: { error: unknown; compact?: boolean }) {
  const { t } = useTranslation('atlas')
  return (
    <div
      role="alert"
      className={cn(
        'flex items-start gap-2 rounded-lg border border-destructive/40 bg-destructive/5 p-3 text-sm',
        !compact && 'm-auto max-w-md',
      )}
    >
      <TriangleAlertIcon className="mt-0.5 size-4 shrink-0 text-destructive" />
      <div className="min-w-0">
        <p className="font-medium text-destructive">{t('preview.failed')}</p>
        <p className="text-xs text-muted-foreground">{translateAtlasWarning(error)}</p>
      </div>
    </div>
  )
}

export interface AtlasViewProps {
  data: PreviewData
  files: ImportedFile[]
  selectedIds: string[]
  onSelectFile: (id: string) => void
  loading?: boolean
  /** Error of the latest rebuild while an older result is still shown. */
  error?: unknown
}

/** Page tabs, zoom/pan canvas with sprite overlays, stats, warnings and merge summary. */
export function AtlasView({ data, files, selectedIds, onSelectFile, loading = false, error }: AtlasViewProps) {
  const { t } = useTranslation('atlas')
  const [pageState, setPage] = useState(0)
  const page = clamp(pageState, 0, Math.max(0, data.pages.length - 1))
  const info = data.pages[page]
  const containerRef = useRef<HTMLDivElement>(null)
  const [view, setView] = useState<View>({ zoom: 1, x: 0, y: 0 })
  const [hovered, setHovered] = useState<string | null>(null)
  const fittedFor = useRef<string>('')
  const drag = useRef<{ startX: number; startY: number; view: View; panning: boolean; id: number } | null>(null)

  const frames = useMemo(() => data.frames.filter((f) => f.page === page), [data.frames, page])
  const selectedPaths = useMemo(() => {
    const ids = new Set(selectedIds)
    return new Set(files.filter((f) => ids.has(f.id)).map((f) => normPath(f.path)))
  }, [files, selectedIds])

  const fit = useCallback(() => {
    const el = containerRef.current
    if (!el || !info) return
    const pad = 16
    const zoom = clamp(Math.min((el.clientWidth - pad * 2) / info.width, (el.clientHeight - pad * 2) / info.height), MIN_ZOOM, MAX_ZOOM) || 1
    setView({ zoom, x: (el.clientWidth - info.width * zoom) / 2, y: (el.clientHeight - info.height * zoom) / 2 })
  }, [info])

  const zoomAt = useCallback((factor: number, cx?: number, cy?: number) => {
    const el = containerRef.current
    setView((v) => {
      const zoom = clamp(v.zoom * factor, MIN_ZOOM, MAX_ZOOM)
      const px = cx ?? (el ? el.clientWidth / 2 : 0)
      const py = cy ?? (el ? el.clientHeight / 2 : 0)
      const k = zoom / v.zoom
      return { zoom, x: px - (px - v.x) * k, y: py - (py - v.y) * k }
    })
  }, [])

  const actualSize = () => {
    const el = containerRef.current
    if (!el || !info) return
    setView({ zoom: 1, x: (el.clientWidth - info.width) / 2, y: (el.clientHeight - info.height) / 2 })
  }

  useEffect(() => {
    const el = containerRef.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const rect = el.getBoundingClientRect()
      zoomAt(e.deltaY < 0 ? 1.15 : 1 / 1.15, e.clientX - rect.left, e.clientY - rect.top)
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [zoomAt])

  // Fit when a page of a new size is shown; keep the user's zoom across refreshes.
  const onImageLoad = () => {
    const key = `${page}:${info?.width}x${info?.height}`
    if (fittedFor.current === key) return
    fittedFor.current = key
    fit()
  }

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    drag.current = { startX: e.clientX, startY: e.clientY, view, panning: false, id: e.pointerId }
  }
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d) return
    const dx = e.clientX - d.startX
    const dy = e.clientY - d.startY
    if (!d.panning && Math.hypot(dx, dy) < DRAG_THRESHOLD) return
    if (!d.panning) {
      d.panning = true
      e.currentTarget.setPointerCapture(d.id)
      setHovered(null)
    }
    setView({ ...d.view, x: d.view.x + dx, y: d.view.y + dy })
  }
  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current
    drag.current = null
    if (d?.panning && e.currentTarget.hasPointerCapture(d.id)) e.currentTarget.releasePointerCapture(d.id)
  }

  const hoveredFrame = hovered ? frames.find((f) => f.name === hovered) ?? null : null
  const counts = data.hasPrevious ? countStatuses(data.plan) : null
  const pct = (v: number) => `${(v * 100).toFixed(1)}%`

  return (
    <div className="flex size-full min-h-0 flex-col gap-2" data-testid="atlas-view">
      <div className="flex shrink-0 flex-wrap items-center gap-1.5">
        {data.pages.length > 1 ? (
          <div role="tablist" aria-label={t('preview.pages')} className="flex items-center gap-1">
            {data.pages.map((_, i) => (
              <button
                key={i}
                type="button"
                role="tab"
                aria-selected={i === page}
                onClick={() => setPage(i)}
                className={cn(
                  'rounded-md px-2 py-1 text-xs font-medium transition-colors',
                  i === page ? 'bg-primary text-primary-foreground' : 'text-muted-foreground hover:bg-accent',
                )}
              >
                {t('preview.page', { index: i + 1 })}
              </button>
            ))}
          </div>
        ) : null}
        {loading ? <Spinner className="size-3.5 text-primary" aria-label={t('preview.loading')} /> : null}
        <div className="ml-auto flex items-center gap-0.5">
          <span className="mr-1 text-xs tabular-nums text-muted-foreground">{`${Math.round(view.zoom * 100)}%`}</span>
          <Tooltip content={t('preview.zoomOut')}>
            <Button variant="ghost" size="icon-sm" aria-label={t('preview.zoomOut')} onClick={() => zoomAt(1 / 1.25)}>
              <ZoomOutIcon />
            </Button>
          </Tooltip>
          <Tooltip content={t('preview.zoomIn')}>
            <Button variant="ghost" size="icon-sm" aria-label={t('preview.zoomIn')} onClick={() => zoomAt(1.25)}>
              <ZoomInIcon />
            </Button>
          </Tooltip>
          <Tooltip content={t('preview.fit')}>
            <Button variant="ghost" size="icon-sm" aria-label={t('preview.fit')} onClick={fit}>
              <Maximize2Icon />
            </Button>
          </Tooltip>
          <Tooltip content={t('preview.actualSize')}>
            <Button variant="ghost" size="icon-sm" aria-label={t('preview.actualSize')} onClick={actualSize}>
              <ScanIcon />
            </Button>
          </Tooltip>
        </div>
      </div>

      {error ? <ErrorNote error={error} compact /> : null}

      <div
        ref={containerRef}
        role="img"
        aria-label={t('preview.canvas')}
        className="relative min-h-0 flex-1 cursor-grab touch-none select-none overflow-hidden rounded-lg border border-border bg-checker active:cursor-grabbing"
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onPointerLeave={() => setHovered(null)}
      >
        {info ? (
          <div
            className="absolute left-0 top-0 origin-top-left"
            style={{ width: info.width, height: info.height, transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})` }}
          >
            <img
              src={data.urls[page]}
              width={info.width}
              height={info.height}
              alt=""
              draggable={false}
              onLoad={onImageLoad}
              className="pointer-events-none absolute inset-0 block max-w-none"
              style={{ imageRendering: view.zoom >= 2 ? 'pixelated' : 'auto' }}
            />
            <svg className="absolute inset-0 overflow-visible" width={info.width} height={info.height} viewBox={`0 0 ${info.width} ${info.height}`}>
              {frames.map((f) => {
                const selected = !!f.sourcePath && selectedPaths.has(normPath(f.sourcePath))
                const active = hovered === f.name
                const status: SpriteStatus | null = f.status
                return (
                  <rect
                    key={f.name}
                    data-testid="atlas-frame"
                    data-name={f.name}
                    data-status={status ?? undefined}
                    data-selected={selected || undefined}
                    aria-label={f.name}
                    x={f.frame.x}
                    y={f.frame.y}
                    width={f.frame.w}
                    height={f.frame.h}
                    vectorEffect="non-scaling-stroke"
                    strokeWidth={active || selected ? 2 : 1}
                    className={cn(
                      'cursor-pointer transition-[fill]',
                      status ? STATUS_STYLES[status].stroke : 'stroke-primary/60',
                      active ? 'fill-primary/25' : selected ? 'fill-primary/15' : 'fill-transparent',
                    )}
                    onPointerEnter={() => setHovered(f.name)}
                    onPointerLeave={() => setHovered((h) => (h === f.name ? null : h))}
                    onClick={() => {
                      const file = fileForFrame(f, files)
                      if (file) onSelectFile(file.id)
                    }}
                  />
                )
              })}
            </svg>
          </div>
        ) : null}
        {hoveredFrame ? <FrameTooltip frame={hoveredFrame} view={view} inList={!!fileForFrame(hoveredFrame, files)} /> : null}
      </div>

      <div className="flex shrink-0 flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground" data-testid="atlas-stats">
        <Stat label={t('preview.stats.size')} value={info ? `${info.width}×${info.height}` : '—'} />
        <Stat label={t('preview.stats.pages')} value={String(data.pages.length)} />
        <Stat label={t('preview.stats.occupancy')} value={pct(data.stats.occupancy)} />
        <Stat label={t('preview.stats.sprites')} value={String(data.stats.spriteCount)} />
        {data.stats.frameCount !== data.stats.spriteCount ? <span>{t('preview.stats.frames', { count: data.stats.frameCount })}</span> : null}
      </div>

      {data.warnings.length > 0 ? (
        <section className="shrink-0 rounded-lg border border-amber-500/40 bg-amber-500/5 p-2" aria-label={t('preview.warnings')}>
          <h3 className="mb-1 flex items-center gap-1.5 text-xs font-semibold text-amber-700 dark:text-amber-300">
            <TriangleAlertIcon className="size-3.5" />
            {t('preview.warnings')}
          </h3>
          <ul className="max-h-24 space-y-0.5 overflow-y-auto text-xs" data-testid="atlas-warnings">
            {data.warnings.map((w, i) => (
              <li key={i}>{translateAtlasWarning(w)}</li>
            ))}
          </ul>
        </section>
      ) : null}

      {counts ? (
        <details className="shrink-0 rounded-lg border border-border p-2 text-xs" data-testid="atlas-plan">
          <summary className="flex cursor-pointer flex-wrap items-center gap-1.5">
            <span className="font-medium">{t('preview.existing')}</span>
            {(['kept', 'replaced', 'new', 'removed'] as const).map((s) =>
              counts[s] > 0 ? <StatusBadge key={s} status={s} count={counts[s]} /> : null,
            )}
          </summary>
          <ul className="mt-2 max-h-40 space-y-0.5 overflow-y-auto">
            {data.plan.map((e) => (
              <li key={e.name} className="flex items-center justify-between gap-2">
                <span className="truncate font-mono">{e.name}</span>
                <StatusBadge status={e.status} />
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </div>
  )
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <span>
      {label}
      {': '}
      <span className="font-medium tabular-nums text-foreground">{value}</span>
    </span>
  )
}

function FrameTooltip({ frame, view, inList }: { frame: AtlasFrame; view: View; inList: boolean }) {
  const { t } = useTranslation('atlas')
  const left = view.x + frame.frame.x * view.zoom
  const top = view.y + (frame.frame.y + frame.frame.h) * view.zoom + 6
  const w = frame.spriteSourceSize.w
  const h = frame.spriteSourceSize.h
  return (
    <div
      role="tooltip"
      className="pointer-events-none absolute z-10 max-w-64 rounded-md bg-foreground px-2.5 py-1.5 text-xs text-background shadow-md"
      style={{ left: Math.max(4, left), top: Math.max(4, top) }}
    >
      <div className="flex items-center gap-1.5">
        <span className="truncate font-semibold">{frame.name}</span>
        {frame.status ? <StatusBadge status={frame.status} /> : null}
      </div>
      <div className="tabular-nums opacity-90">{t('preview.tooltip.size', { w, h })}</div>
      {frame.trimmed ? <div className="tabular-nums opacity-75">{t('preview.tooltip.source', { w: frame.sourceSize.w, h: frame.sourceSize.h })}</div> : null}
      {frame.rotated || frame.trimmed ? (
        <div className="opacity-75">
          {[frame.rotated ? t('preview.tooltip.rotated') : null, frame.trimmed ? t('preview.tooltip.trimmed') : null].filter(Boolean).join(' · ')}
        </div>
      ) : null}
      {frame.aliases.length > 0 ? <div className="opacity-75">{t('preview.tooltip.aliases', { names: frame.aliases.join(', ') })}</div> : null}
      <div className="mt-0.5 opacity-60">{inList ? t('preview.tooltip.select') : t('preview.tooltip.notInList')}</div>
    </div>
  )
}
