// Debounced data hooks of the atlas tab (preview build + existing project).
import { useEffect, useMemo, useState } from 'react'
import { create } from 'zustand'

import type { ImportedFile } from '@/lib/ipc/types'
import type { Params } from '@/stores/session'

import { atlasLoadProject, atlasPreview, type AtlasPreviewResult, type AtlasProjectSummary } from './ipc'
import { baseNameProblem, buildAtlasRequest, projectFilePath } from './schema'

export const PREVIEW_DEBOUNCE_MS = 300

/** Bumped after each export so previews / summaries re-read the output folder. */
interface AtlasRefreshStore {
  versions: Record<string, number>
  bump: (tabId: string) => void
}

export const useAtlasRefresh = create<AtlasRefreshStore>()((set) => ({
  versions: {},
  bump: (tabId) => set((s) => ({ versions: { ...s.versions, [tabId]: (s.versions[tabId] ?? 0) + 1 } })),
}))

export interface PreviewData extends AtlasPreviewResult {
  /** Object URL per page PNG (revoked when replaced or unmounted). */
  urls: string[]
}

export interface PreviewState {
  data: PreviewData | null
  error: unknown
  loading: boolean
}

function pngUrl(png: ArrayBuffer): string {
  return typeof URL.createObjectURL === 'function' ? URL.createObjectURL(new Blob([png], { type: 'image/png' })) : ''
}

/** Rebuilds the atlas preview (debounced) whenever files, params or the output change. */
export function useAtlasPreview(tabId: string, files: ImportedFile[], params: Params, delay = PREVIEW_DEBOUNCE_MS): PreviewState {
  const version = useAtlasRefresh((s) => s.versions[tabId] ?? 0)
  const paths = useMemo(() => files.map((f) => f.path), [files])
  const request = useMemo(() => buildAtlasRequest(params), [params])
  const key = useMemo(() => JSON.stringify([paths, request, version]), [paths, request, version])
  const enabled = paths.length > 0
  const [result, setResult] = useState<{ key: string; data: PreviewData | null; error: unknown } | null>(null)

  useEffect(() => {
    if (!enabled) return
    let cancelled = false
    const timer = setTimeout(() => {
      atlasPreview(tabId, paths, request).then(
        (res) => {
          if (cancelled) return
          setResult({ key, data: { ...res, urls: res.pages.map((p) => pngUrl(p.png)) }, error: null })
        },
        (error: unknown) => {
          if (!cancelled) setResult((prev) => ({ key, data: prev?.data ?? null, error }))
        },
      )
    }, delay)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [key, enabled, tabId, paths, request, delay])

  // Free the page blobs of a result once it is replaced (or on unmount).
  const data = result?.data ?? null
  useEffect(() => {
    const urls = data?.urls ?? []
    return () => urls.forEach((u) => u && URL.revokeObjectURL?.(u))
  }, [data])

  if (!enabled) return { data: null, error: null, loading: false }
  return {
    data,
    error: result?.key === key ? result.error : null,
    loading: result?.key !== key,
  }
}

export interface ExistingAtlasState {
  summary: AtlasProjectSummary | null
  /** The project file looked up (null when the output is incomplete). */
  path: string | null
  loading: boolean
}

/** Looks up `<outputDir>/<baseName>.texatlas.json` and the merge plan for the current files. */
export function useExistingAtlas(tabId: string, files: ImportedFile[], params: Params, delay = PREVIEW_DEBOUNCE_MS): ExistingAtlasState {
  const version = useAtlasRefresh((s) => s.versions[tabId] ?? 0)
  const outputDir = typeof params.outputDir === 'string' ? params.outputDir.trim() : ''
  const baseName = typeof params.baseName === 'string' ? params.baseName : ''
  const removeMissing = params.removeMissing === true
  const path = outputDir && !baseNameProblem(baseName) ? projectFilePath(outputDir, baseName) : null
  const paths = useMemo(() => files.map((f) => f.path), [files])
  const key = JSON.stringify([path, paths, removeMissing, version])
  const [result, setResult] = useState<{ key: string; path: string; summary: AtlasProjectSummary | null } | null>(null)

  useEffect(() => {
    if (!path) return
    let cancelled = false
    const timer = setTimeout(() => {
      atlasLoadProject(path, paths, removeMissing).then(
        (summary) => {
          if (!cancelled) setResult({ key, path, summary })
        },
        () => {
          if (!cancelled) setResult({ key, path, summary: null })
        },
      )
    }, delay)
    return () => {
      cancelled = true
      clearTimeout(timer)
    }
  }, [key, path, paths, removeMissing, delay])

  if (!path) return { summary: null, path: null, loading: false }
  // Keep showing the last summary of the same project while the plan refreshes.
  return { summary: result?.path === path ? result.summary : null, path, loading: result?.key !== key }
}
