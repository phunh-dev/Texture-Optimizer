// Debounced layout preview (`mesh_preview_pack`) + UV status prediction.
import { useEffect, useMemo, useState } from 'react'

import { toAppError } from '@/lib/errors'
import type { ImportedFile } from '@/lib/ipc/types'
import type { Params } from '@/stores/session'

import { meshPreviewPack, type MaterialInfo, type MaterialStatus, type PreviewPayload } from './ipc'
import { buildPackOptions, resolveParams } from './schema'

export const PREVIEW_DEBOUNCE_MS = 400
const UV_EPSILON = 1e-4

export interface PreviewState {
  data: PreviewPayload | null
  /** `data:` URL per page image. */
  urls: string[]
  error: unknown
  loading: boolean
}

export function pageUrl(png: string): string {
  return `data:image/png;base64,${png}`
}

/** Rebuilds the preview whenever the models or the packing params change. */
export function usePackPreview(tabId: string, files: ImportedFile[], params: Params, delay = PREVIEW_DEBOUNCE_MS): PreviewState {
  const paths = useMemo(() => files.map((f) => f.path), [files])
  // Output-only params do not change the layout.
  const options = useMemo(() => {
    const o = buildPackOptions(params)
    return { ...o, output: { ...o.output, mode: 'rewriteModels' as const } }
  }, [params])
  const key = useMemo(() => JSON.stringify([paths, { ...options, output: null }]), [paths, options])
  const enabled = paths.length > 0
  const [result, setResult] = useState<{ key: string; data: PreviewPayload | null; error: unknown } | null>(null)

  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    const timer = setTimeout(() => {
      meshPreviewPack(tabId, paths, options).then(
        (data) => {
          if (!cancelled) setResult({ key, data, error: null })
        },
        (error: unknown) => {
          if (cancelled || toAppError(error).code === 'CANCELLED') return
          setResult((prev) => ({ key, data: prev?.data ?? null, error }))
        },
      )
    }, delay)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
    // `key` captures paths + options.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, enabled, tabId, delay])

  const data = enabled ? (result?.data ?? null) : null
  const urls = useMemo(() => (data ? data.images.map((i) => pageUrl(i.png)) : []), [data])
  return {
    data,
    urls,
    error: enabled && result?.key === key ? result.error : null,
    loading: enabled && result?.key !== key,
  }
}

/** Integer tile block covering a UV range (same as Rust `repeat_tiles`). */
export function repeatTiles(min: [number, number], max: [number, number]): [number, number] {
  const t = (a: number) => Math.max(1, Math.ceil(max[a] - UV_EPSILON) - Math.floor(min[a] + UV_EPSILON))
  return [t(0), t(1)]
}

/** What the packer will do with a material under the current params (before any preview). */
export function predictStatus(material: MaterialInfo, params: Params): { status: MaterialStatus; tiles: [number, number] } {
  const p = resolveParams(params)
  const loadable = material.textures.some((t) => t.uvChannel === material.uvChannel && (t.exists || t.embedded))
  if (material.uvChannel == null || !loadable) return { status: 'noTextures', tiles: [1, 1] }
  const range = material.uvRange
  if (!range) return { status: 'noUvs', tiles: [1, 1] }
  if (!range.outOfRange) return { status: 'inRange', tiles: [1, 1] }
  switch (p.outOfRange) {
    case 'skipMaterial':
      return { status: 'skipped', tiles: [1, 1] }
    case 'clamp':
      return { status: 'clamped', tiles: [1, 1] }
    case 'wrapIntoTile':
      return { status: 'wrapped', tiles: [1, 1] }
    case 'bakeRepeat': {
      const tiles = repeatTiles(range.min, range.max)
      return tiles[0] > p.maxTiles || tiles[1] > p.maxTiles ? { status: 'tooManyTiles', tiles } : { status: 'repeated', tiles }
    }
  }
}

export const PACKED_STATUSES: readonly MaterialStatus[] = ['inRange', 'clamped', 'wrapped', 'repeated']
