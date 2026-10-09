import { Maximize2Icon, ScanIcon, ZoomInIcon, ZoomOutIcon } from 'lucide-react'
import { useCallback, useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/misc'
import { Switch } from '@/components/ui/switch'
import { Tooltip } from '@/components/ui/tooltip'
import { clamp, cn } from '@/lib/utils'

export interface CompareViewProps {
  /** Image URLs (thumb:// URL, blob: URL from a PreviewResult, ...). */
  before: string
  after: string
  className?: string
  /** Start with nearest-neighbour rendering (pixel art). */
  defaultPixelated?: boolean
  /** Initial divider position 0..1. */
  defaultSplit?: number
  /** Eyedropper mode: a click reports image pixel coordinates (via onPickBefore) instead of panning. */
  pickMode?: boolean
  /**
   * Called in pickMode with integer pixel coordinates in the image's natural
   * size (zoom/pan already undone). Clicks outside the image are ignored.
   */
  onPickBefore?: (x: number, y: number) => void
}

const MIN_ZOOM = 0.05
const MAX_ZOOM = 64

interface View {
  zoom: number
  x: number
  y: number
}

/**
 * Before/after comparison: draggable divider, wheel zoom around the cursor,
 * drag to pan, checkerboard background and a pixelated rendering toggle.
 */
export function CompareView({
  before,
  after,
  className,
  defaultPixelated = false,
  defaultSplit = 0.5,
  pickMode = false,
  onPickBefore,
}: CompareViewProps) {
  const { t } = useTranslation('common')
  const containerRef = useRef<HTMLDivElement>(null)
  const [natural, setNatural] = useState<{ w: number; h: number } | null>(null)
  const [view, setView] = useState<View>({ zoom: 1, x: 0, y: 0 })
  const [split, setSplit] = useState(clamp(defaultSplit, 0, 1))
  const [pixelated, setPixelated] = useState(defaultPixelated)
  const drag = useRef<{ kind: 'pan' | 'split'; startX: number; startY: number; view: View } | null>(null)

  const fitTo = useCallback((size: { w: number; h: number } | null) => {
    const el = containerRef.current
    if (!el || !size) return
    const pad = 24
    const zoom = clamp(Math.min((el.clientWidth - pad * 2) / size.w, (el.clientHeight - pad * 2) / size.h), MIN_ZOOM, MAX_ZOOM) || 1
    setView({ zoom, x: (el.clientWidth - size.w * zoom) / 2, y: (el.clientHeight - size.h * zoom) / 2 })
  }, [])
  const fit = () => fitTo(natural)

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
    if (!el || !natural) return
    setView({ zoom: 1, x: (el.clientWidth - natural.w) / 2, y: (el.clientHeight - natural.h) / 2 })
  }

  // Non-passive wheel listener so the page does not scroll while zooming.
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

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>, kind: 'pan' | 'split') => {
    if (e.button !== 0) return
    e.stopPropagation()
    if (kind === 'pan' && pickMode && onPickBefore) {
      const el = containerRef.current
      if (!el) return
      const rect = el.getBoundingClientRect()
      const x = Math.floor((e.clientX - rect.left - view.x) / view.zoom)
      const y = Math.floor((e.clientY - rect.top - view.y) / view.zoom)
      if (x < 0 || y < 0 || (natural && (x >= natural.w || y >= natural.h))) return
      onPickBefore(x, y)
      return
    }
    e.currentTarget.setPointerCapture?.(e.pointerId)
    drag.current = { kind, startX: e.clientX, startY: e.clientY, view }
  }

  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current
    const el = containerRef.current
    if (!d || !el) return
    if (d.kind === 'pan') {
      setView({ ...d.view, x: d.view.x + e.clientX - d.startX, y: d.view.y + e.clientY - d.startY })
    } else {
      const rect = el.getBoundingClientRect()
      if (rect.width > 0) setSplit(clamp((e.clientX - rect.left) / rect.width, 0, 1))
    }
  }

  const endDrag = () => {
    drag.current = null
  }

  const layerStyle = {
    transform: `translate(${view.x}px, ${view.y}px) scale(${view.zoom})`,
    transformOrigin: '0 0',
    imageRendering: pixelated ? ('pixelated' as const) : ('auto' as const),
  }

  return (
    <div className={cn('flex size-full flex-col overflow-hidden rounded-lg border border-border bg-card', className)} data-testid="compare-view">
      <div
        ref={containerRef}
        role="img"
        aria-label={t('compare.label')}
        data-pick-mode={pickMode || undefined}
        className={cn(
          'bg-checker relative min-h-0 flex-1 touch-none overflow-hidden',
          pickMode ? 'cursor-crosshair' : 'cursor-grab active:cursor-grabbing',
        )}
        onPointerDown={(e) => onPointerDown(e, 'pan')}
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
        onDoubleClick={fit}
      >
        <div className="pointer-events-none absolute inset-0">
          <img
            src={before}
            alt=""
            draggable={false}
            className="absolute left-0 top-0 max-w-none"
            style={layerStyle}
            onLoad={(e) => {
              const size = { w: e.currentTarget.naturalWidth || 1, h: e.currentTarget.naturalHeight || 1 }
              setNatural(size)
              fitTo(size)
            }}
          />
        </div>
        <div className="pointer-events-none absolute inset-0" style={{ clipPath: `inset(0 0 0 ${split * 100}%)` }}>
          <img src={after} alt="" draggable={false} className="absolute left-0 top-0 max-w-none" style={layerStyle} />
        </div>

        <span className="pointer-events-none absolute left-3 top-3 rounded-full bg-black/55 px-2 py-0.5 text-[11px] font-medium text-white backdrop-blur">
          {t('compare.before')}
        </span>
        <span className="pointer-events-none absolute right-3 top-3 rounded-full bg-black/55 px-2 py-0.5 text-[11px] font-medium text-white backdrop-blur">
          {t('compare.after')}
        </span>

        <div
          role="slider"
          tabIndex={0}
          aria-label={t('compare.divider')}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(split * 100)}
          className="group/divider absolute inset-y-0 z-10 flex w-6 -translate-x-1/2 cursor-ew-resize justify-center outline-none"
          style={{ left: `${split * 100}%` }}
          onPointerDown={(e) => onPointerDown(e, 'split')}
          onKeyDown={(e) => {
            if (e.key === 'ArrowLeft') setSplit((s) => clamp(s - 0.02, 0, 1))
            if (e.key === 'ArrowRight') setSplit((s) => clamp(s + 0.02, 0, 1))
          }}
        >
          <div className="h-full w-0.5 bg-white shadow-[0_0_0_1px_rgba(0,0,0,0.25)]" />
          <div className="absolute top-1/2 flex size-7 -translate-y-1/2 items-center justify-center rounded-full border border-black/10 bg-white text-neutral-700 shadow-lg transition-transform group-hover/divider:scale-110 group-focus-visible/divider:ring-2 group-focus-visible/divider:ring-ring">
            <svg viewBox="0 0 16 16" className="size-3.5" fill="currentColor" aria-hidden>
              <path d="M6 3 1 8l5 5V3Zm4 0v10l5-5-5-5Z" />
            </svg>
          </div>
        </div>
      </div>

      <div className="flex items-center gap-1 border-t border-border px-2 py-1.5">
        <Tooltip content={t('compare.zoomOut')}>
          <Button variant="ghost" size="icon-xs" aria-label={t('compare.zoomOut')} onClick={() => zoomAt(1 / 1.25)}>
            <ZoomOutIcon />
          </Button>
        </Tooltip>
        <span className="w-12 text-center text-xs tabular-nums text-muted-foreground">{t('compare.zoom', { value: Math.round(view.zoom * 100) })}</span>
        <Tooltip content={t('compare.zoomIn')}>
          <Button variant="ghost" size="icon-xs" aria-label={t('compare.zoomIn')} onClick={() => zoomAt(1.25)}>
            <ZoomInIcon />
          </Button>
        </Tooltip>
        <Separator orientation="vertical" className="mx-1" />
        <Tooltip content={t('compare.fit')}>
          <Button variant="ghost" size="icon-xs" aria-label={t('compare.fit')} onClick={fit}>
            <Maximize2Icon />
          </Button>
        </Tooltip>
        <Tooltip content={t('compare.actualSize')}>
          <Button variant="ghost" size="icon-xs" aria-label={t('compare.actualSize')} onClick={actualSize}>
            <ScanIcon />
          </Button>
        </Tooltip>
        <div className="ml-auto flex items-center gap-2">
          <label htmlFor="compare-pixelated" className="text-xs text-muted-foreground">
            {t('compare.pixelated')}
          </label>
          <Switch id="compare-pixelated" checked={pixelated} onCheckedChange={setPixelated} />
        </div>
      </div>
    </div>
  )
}
