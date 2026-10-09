import { AlertTriangleIcon, ImageIcon } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'

import { CompareView } from '@/components/ToolLayout'
import { Spinner } from '@/components/ui/misc'
import { useLooseT } from '@/i18n/loose'
import { inTauri } from '@/lib/env'
import { translateError } from '@/lib/errors'
import { previewOp, thumbnailUrl } from '@/lib/ipc'
import type { ImportedFile, OpRequest } from '@/lib/ipc/types'
import { requireSession, useSession } from '@/stores/session'

import { PICK_FLAG } from './fields'
import { createPixelSampler, type PixelSampler } from './sampler'
import { buildBgRemoveRequest, toHex } from './schema'

export const PREVIEW_DEBOUNCE_MS = 250

/**
 * Identity request: the Resolution Fixer targeting a multiple of 1 returns the
 * input unchanged. Used to get the full-resolution original as PNG (the webview
 * cannot show TGA/BMP/... directly, and thumbnails are downscaled), both for
 * the "before" layer and for eyedropper sampling.
 */
export const ORIGINAL_REQUEST: OpRequest = { kind: 'resolution', params: { target: 'multipleOfN', n: 1, maxSize: 0 } }

export interface BgRemoveMeta {
  removedPixels: number
  detectedMode: 'white' | 'checker' | 'color' | 'none'
  checkerCellSize: number | null
  bgColors: [number, number, number, number][]
}

interface Rendered {
  url: string
  width: number
  height: number
  meta: BgRemoveMeta | null
}

interface Original {
  url: string
  blob: Blob
}

function useObjectUrlCleanup(url: string | undefined) {
  useEffect(() => {
    if (!url) return
    return () => URL.revokeObjectURL(url)
  }, [url])
}

export function BgRemovePreview({ tabId, file }: { tabId: string; file: ImportedFile | null }) {
  const { t } = useTranslation('bgremove')
  const params = useSession(tabId, (s) => s.params)
  const picking = useSession(tabId, (s) => s.uiFlags[PICK_FLAG] === true)
  const requestKey = useMemo(() => JSON.stringify(buildBgRemoveRequest(params)), [params])
  const path = file?.path ?? null
  const mtime = file?.mtimeMs ?? 0

  const [original, setOriginal] = useState<Original | null>(null)
  const [result, setResult] = useState<Rendered | null>(null)
  const [error, setError] = useState<unknown>(null)
  const [loading, setLoading] = useState(false)
  const sampler = useRef<{ url: string; sampler: PixelSampler } | null>(null)
  const seq = useRef(0)

  useObjectUrlCleanup(original?.url)
  useObjectUrlCleanup(result?.url)

  // Full-resolution original (once per file).
  useEffect(() => {
    if (!path) return
    let cancelled = false
    previewOp(tabId, path, ORIGINAL_REQUEST)
      .then((r) => {
        if (cancelled) return
        const blob = new Blob([r.png], { type: 'image/png' })
        setOriginal({ url: URL.createObjectURL(blob), blob })
      })
      .catch(() => {
        // The processed preview reports load errors; the thumbnail stays as "before".
        if (!cancelled) setOriginal(null)
      })
    return () => {
      cancelled = true
    }
  }, [tabId, path, mtime])

  // Processed image, debounced on every param change.
  useEffect(() => {
    if (!path) return
    const id = ++seq.current
    const timer = setTimeout(() => {
      setLoading(true)
      previewOp(tabId, path, JSON.parse(requestKey) as OpRequest)
        .then((r) => {
          if (id !== seq.current) return
          setResult({
            url: URL.createObjectURL(new Blob([r.png], { type: 'image/png' })),
            width: r.width,
            height: r.height,
            meta: (r.meta as BgRemoveMeta | null) ?? null,
          })
          setError(null)
        })
        .catch((err: unknown) => {
          if (id !== seq.current) return
          setResult(null)
          setError(err)
        })
        .finally(() => {
          if (id === seq.current) setLoading(false)
        })
    }, PREVIEW_DEBOUNCE_MS)
    return () => clearTimeout(timer)
  }, [tabId, path, mtime, requestKey])

  useEffect(
    () => () => {
      sampler.current?.sampler.dispose()
      sampler.current = null
    },
    [original],
  )

  if (!file) {
    return (
      <div className="flex size-full flex-col items-center justify-center gap-2 rounded-lg border border-dashed border-border text-sm text-muted-foreground">
        <ImageIcon className="size-6" />
        {t('preview.noFile')}
      </div>
    )
  }

  const pickEnabled = picking && params.mode === 'color' && original !== null

  const onPick = async (x: number, y: number) => {
    if (!original) return
    try {
      if (!sampler.current || sampler.current.url !== original.url) {
        sampler.current?.sampler.dispose()
        sampler.current = { url: original.url, sampler: await createPixelSampler(original.blob) }
      }
      const px = sampler.current.sampler.sample(x, y)
      if (!px) return
      const s = requireSession(tabId).getState()
      s.setParams({ color: [px[0], px[1], px[2], 255] })
      s.setUiFlag(PICK_FLAG, false)
    } catch (err) {
      toast.error(translateError(err))
    }
  }

  const before = original?.url ?? (inTauri() ? thumbnailUrl(file, 'large') : null)
  return (
    <div className="flex size-full flex-col gap-2" data-testid="bgremove-preview">
      <div className="relative min-h-0 flex-1">
        {before ? (
          <CompareView before={before} after={result?.url ?? before} pickMode={pickEnabled} onPickBefore={(x, y) => void onPick(x, y)} />
        ) : (
          <div className="bg-checker flex size-full items-center justify-center rounded-lg border border-border">
            <Spinner className="size-5 text-muted-foreground" />
          </div>
        )}
        {loading ? (
          <div className="pointer-events-none absolute right-3 top-10 flex items-center gap-1.5 rounded-full bg-black/55 px-2 py-0.5 text-[11px] text-white" role="status">
            <Spinner className="size-3" />
            {t('preview.loading')}
          </div>
        ) : null}
      </div>
      <PreviewInfo result={result} error={error} />
    </div>
  )
}

function PreviewInfo({ result, error }: { result: Rendered | null; error: unknown }) {
  const { t } = useTranslation('bgremove')
  const lt = useLooseT()
  if (error) {
    return (
      <div role="alert" className="flex items-center gap-2 rounded-md border border-destructive/40 bg-destructive/10 px-3 py-2 text-xs text-destructive">
        <AlertTriangleIcon className="size-4 shrink-0" />
        {translateError(error)}
      </div>
    )
  }
  const meta = result?.meta
  if (!result || !meta) return null
  const total = Math.max(1, result.width * result.height)
  const percent = ((meta.removedPixels / total) * 100).toFixed(1)
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-1 rounded-md border border-border bg-card px-3 py-2 text-xs" data-testid="bgremove-meta">
      <span>
        <span className="text-muted-foreground">{t('preview.detected')}</span>{' '}
        <span className="font-medium">{lt(`bgremove:preview.detectedModes.${meta.detectedMode}`)}</span>
      </span>
      {meta.checkerCellSize != null ? <span>{t('preview.cell', { size: meta.checkerCellSize })}</span> : null}
      {meta.bgColors.length > 0 ? (
        <span className="flex items-center gap-1.5">
          <span className="text-muted-foreground">{t('preview.colors')}</span>
          {meta.bgColors.map((c, i) => {
            const hex = toHex(c)
            return (
              <span
                key={`${hex}-${i}`}
                role="img"
                aria-label={t('preview.swatch', { hex: hex.toUpperCase() })}
                title={hex.toUpperCase()}
                className="inline-block size-4 rounded-sm border border-border"
                style={{ backgroundColor: hex }}
              />
            )
          })}
        </span>
      ) : null}
      <span className="ml-auto tabular-nums font-medium">{t('preview.removed', { percent })}</span>
    </div>
  )
}
