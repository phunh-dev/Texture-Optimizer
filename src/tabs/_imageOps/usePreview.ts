import { useEffect, useState } from 'react'

import { toAppError } from '@/lib/errors'
import { previewOp } from '@/lib/ipc'
import type { AppError, ImportedFile, OpRequest, PreviewResult } from '@/lib/ipc/types'

/** Delay between the last param / focus change and the preview request. */
export const PREVIEW_DEBOUNCE_MS = 250

export interface PreviewState {
  /** True while a newer preview than the shown one is pending (debounce + request). */
  loading: boolean
  /**
   * Latest successful result for the focused file. While a newer request is pending this is
   * the previous result of the same file (`stale`), so the view does not flash.
   */
  result: PreviewResult | null
  stale: boolean
  /** Object URL of `result.png`. */
  url: string | null
  error: AppError | null
  /** Identifies the (tab, file, request) the shown result belongs to. */
  key: string | null
}

interface Settled {
  key: string
  path: string
  result: PreviewResult | null
  url: string | null
  error: AppError | null
}

/** Stable identity of a preview: changes when the file, its mtime or the request change. */
export function previewKey(tabId: string, file: Pick<ImportedFile, 'path' | 'mtimeMs'> | null, request: OpRequest | null): string | null {
  if (!file || !request) return null
  return JSON.stringify({ tabId, path: file.path, m: file.mtimeMs, request })
}

function objectUrl(png: ArrayBuffer): string | null {
  if (typeof URL === 'undefined' || typeof URL.createObjectURL !== 'function') return null
  return URL.createObjectURL(new Blob([png], { type: 'image/png' }))
}

/**
 * Debounced in-memory preview of `request` on `file` (via `previewOp`). Stale
 * responses are ignored; object URLs are revoked when replaced or unmounted.
 */
export function useOpPreview(tabId: string, file: ImportedFile | null, request: OpRequest | null): PreviewState {
  const key = previewKey(tabId, file, request)
  const [settled, setSettled] = useState<Settled | null>(null)

  useEffect(() => {
    if (!key) return
    // Everything the request needs is encoded in the key (keeps the effect keyed on it only).
    const { tabId: tab, path, request: req } = JSON.parse(key) as { tabId: string; path: string; request: OpRequest }
    let cancelled = false
    const timer = setTimeout(() => {
      previewOp(tab, path, req).then(
        (result) => {
          if (!cancelled) setSettled({ key, path, result, url: objectUrl(result.png), error: null })
        },
        (err: unknown) => {
          if (!cancelled) setSettled({ key, path, result: null, url: null, error: toAppError(err) })
        },
      )
    }, PREVIEW_DEBOUNCE_MS)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [key])

  const url = settled?.url ?? null
  useEffect(
    () => () => {
      if (url) URL.revokeObjectURL(url)
    },
    [url],
  )

  if (!key || !settled) return { loading: !!key, result: null, stale: false, url: null, error: null, key: null }
  const current = settled.key === key
  // A pending request keeps showing the previous result of the same file.
  const keep = current || settled.path === file?.path
  return {
    loading: !current,
    result: keep ? settled.result : null,
    stale: !current && keep && settled.result !== null,
    url: keep ? settled.url : null,
    error: current ? settled.error : null,
    key: keep ? settled.key : null,
  }
}
