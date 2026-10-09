// Typed wrappers around the atlas commands (src-tauri/src/commands_atlas.rs).
import { invoke } from '@tauri-apps/api/core'

import type { AppError } from '@/lib/ipc/types'

import type { AtlasParamsJson, AtlasRequest } from './schema'

export interface Rect {
  x: number
  y: number
  w: number
  h: number
}

export type SpriteStatus = 'new' | 'replaced' | 'kept' | 'removed'

export interface PlanEntry {
  name: string
  status: SpriteStatus
  sourcePath: string | null
}

export interface AtlasFrame {
  name: string
  aliases: string[]
  page: number
  /** Occupied area in the page (h×w of the trimmed sprite when rotated). */
  frame: Rect
  rotated: boolean
  trimmed: boolean
  sourceSize: { w: number; h: number }
  spriteSourceSize: Rect
  sourcePath: string | null
  /** Merge status (null when no atlas exists at the output yet). */
  status: SpriteStatus | null
}

export interface PageStats {
  width: number
  height: number
  spriteCount: number
  usedArea: number
  occupancy: number
}

export interface AtlasStats {
  pages: PageStats[]
  spriteCount: number
  frameCount: number
  occupancy: number
}

export interface AtlasPreviewHeader {
  pages: { width: number; height: number; byteLength: number }[]
  frames: AtlasFrame[]
  stats: AtlasStats
  warnings: AppError[]
  plan: PlanEntry[]
  hasPrevious: boolean
  params: AtlasParamsJson
}

export interface AtlasPreviewResult extends Omit<AtlasPreviewHeader, 'pages'> {
  pages: { width: number; height: number; png: ArrayBuffer }[]
}

/**
 * Decode the binary `atlas_preview` payload: little-endian u32 `headerLen`,
 * the UTF-8 JSON header, then each page PNG (`header.pages[i].byteLength`).
 */
export function decodeAtlasPreview(payload: ArrayBuffer | Uint8Array | number[]): AtlasPreviewResult {
  const bytes =
    payload instanceof Uint8Array ? payload : Array.isArray(payload) ? Uint8Array.from(payload) : new Uint8Array(payload)
  if (bytes.byteLength < 4) throw new Error('atlas preview payload too short')
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const headerLen = view.getUint32(0, true)
  const header = JSON.parse(new TextDecoder().decode(bytes.subarray(4, 4 + headerLen))) as AtlasPreviewHeader
  let offset = 4 + headerLen
  const pages = header.pages.map((p) => {
    const start = bytes.byteOffset + offset
    const png = bytes.buffer.slice(start, start + p.byteLength) as ArrayBuffer
    offset += p.byteLength
    return { width: p.width, height: p.height, png }
  })
  if (offset !== bytes.byteLength) throw new Error('atlas preview payload size mismatch')
  return { ...header, pages }
}

/** Build the atlas in memory (merged with the atlas at outputDir/baseName when given). */
export async function atlasPreview(tabId: string, paths: string[], req: AtlasRequest): Promise<AtlasPreviewResult> {
  const payload = await invoke<ArrayBuffer | number[]>('atlas_preview', {
    tabId,
    paths,
    params: req.params,
    exporter: req.exporter,
    incrementalOptions: req.incrementalOptions,
    outputDir: req.outputDir || null,
    baseName: req.baseName || null,
  })
  return decodeAtlasPreview(payload)
}

/** Start the export job; resolves with the job id (progress via job events). */
export function atlasExport(tabId: string, paths: string[], req: AtlasRequest): Promise<string> {
  return invoke('atlas_export', {
    tabId,
    paths,
    params: req.params,
    exporter: req.exporter,
    outputDir: req.outputDir,
    baseName: req.baseName,
    incrementalOptions: req.incrementalOptions,
  })
}

export interface AtlasProjectSummary {
  path: string
  pages: { width: number; height: number }[]
  spriteCount: number
  frameCount: number
  exporter: string | null
  params: AtlasParamsJson
  stats: AtlasStats
  plan: PlanEntry[]
}

/** Existing project at `path` (null when absent) and the merge plan for `paths`. */
export function atlasLoadProject(path: string, paths: string[], removeMissing: boolean): Promise<AtlasProjectSummary | null> {
  return invoke('atlas_load_project', { path, paths, removeMissing })
}

/** `meta` of the first `job://finished` result of an atlas export. */
export interface AtlasExportSummary {
  kind: 'atlasExport'
  outputDir: string
  projectPath: string
  written: string[]
  deleted: string[]
  warnings: AppError[]
  stats: AtlasStats
  plan: PlanEntry[]
}

export function isExportSummary(meta: unknown): meta is AtlasExportSummary {
  return !!meta && typeof meta === 'object' && (meta as { kind?: unknown }).kind === 'atlasExport'
}

/** Counts per status. */
export function countStatuses(plan: PlanEntry[]): Record<SpriteStatus, number> {
  const counts: Record<SpriteStatus, number> = { new: 0, replaced: 0, kept: 0, removed: 0 }
  for (const e of plan) counts[e.status]++
  return counts
}
