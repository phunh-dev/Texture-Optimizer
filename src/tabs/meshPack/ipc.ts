// IPC of the 3D Texture Packer (Rust: src-tauri/src/commands_mesh.rs,
// shapes from crates/texopt-core/src/mesh/pack). Change both sides together.
import { invoke } from '@tauri-apps/api/core'

import type { AppError, ImportedFile } from '@/lib/ipc/types'

import type { PackOptionsJson } from './schema'

export type ModelFormat = 'fbx' | 'obj' | 'dae'

export interface UvRange {
  min: [number, number]
  max: [number, number]
  outOfRange: boolean
}

export interface TextureInfo {
  /** 'baseColor', 'normal', … or 'other:<name>'. */
  channel: string
  path: string
  rawPath: string
  exists: boolean
  embedded: boolean
  uvChannel: number
  width: number | null
  height: number | null
  mtimeMs: number
}

export interface MaterialInfo {
  index: number
  name: string
  meshCount: number
  vertexCount: number
  uvChannel: number | null
  uvRange: UvRange | null
  textures: TextureInfo[]
}

export interface ModelInfo {
  path: string
  format: ModelFormat
  meshCount: number
  vertexCount: number
  embeddedTextureCount: number
  materials: MaterialInfo[]
  warnings: AppError[]
}

export interface ScannedModel {
  /** Same shape as image entries (width/height are 0). */
  file: ImportedFile
  info: ModelInfo
}

export interface MeshScanResult {
  models: ScannedModel[]
  skipped: { path: string; error: AppError }[]
}

export type MaterialStatus = 'inRange' | 'clamped' | 'wrapped' | 'repeated' | 'skipped' | 'tooManyTiles' | 'noUvs' | 'noTextures'
export type ModelOutcome = 'rewritten' | 'fallback' | 'remapData' | 'skipped' | 'failed'

export interface AtlasRect {
  x: number
  y: number
  width: number
  height: number
}

export interface MaterialReport {
  materialIndex: number
  name: string
  status: MaterialStatus
  uvChannel: number | null
  uvRange: UvRange | null
  channels: string[]
  layoutChannel: string | null
  sourceSize: [number, number] | null
  tileSize: [number, number] | null
  tiles: [number, number]
  page: number | null
  rect: AtlasRect | null
  remap: { normalize: unknown; transform: { offset: [number, number]; scale: [number, number] } } | null
}

export interface ModelReport {
  source: string
  name: string
  format: ModelFormat | null
  outcome: ModelOutcome
  output: string | null
  outputFormat: string | null
  sidecar: string | null
  files: string[]
  pages: number[]
  materials: MaterialReport[]
  warnings: AppError[]
  error: AppError | null
}

export interface PageReport {
  index: number
  width: number
  height: number
  occupancy: number
  textures: { channel: string; path: string }[]
}

export interface PackReport {
  version: number
  generator: string
  uvOrigin: 'bottomLeft' | 'topLeft'
  scalePercent: number
  channels: string[]
  pages: PageReport[]
  models: ModelReport[]
  files: string[]
  reportPath: string | null
  warnings: AppError[]
}

export interface PreviewPayload {
  report: PackReport
  /** Channel shown by the preview images (base colour when present). */
  channel: string
  /** One per page: real page size + base64 PNG (downscaled to ≤1024 px). */
  images: { width: number; height: number; png: string }[]
}

/** Summary entry appended to the `job://finished` results of `mesh_pack`. */
export interface PackSummaryMeta {
  kind: 'summary'
  rewritten?: number
  fallback?: number
  remapData?: number
  skipped?: number
  failed?: number
  pages?: number
  files?: string[]
  warnings?: AppError[]
}

export interface PackModelMeta {
  kind: 'model'
  outcome: ModelOutcome
  sidecar: string | null
  files: string[]
  warnings: AppError[]
}

export function isSummaryMeta(meta: unknown): meta is PackSummaryMeta {
  return !!meta && typeof meta === 'object' && (meta as { kind?: unknown }).kind === 'summary'
}

export function isModelMeta(meta: unknown): meta is PackModelMeta {
  return !!meta && typeof meta === 'object' && (meta as { kind?: unknown }).kind === 'model'
}

export function meshScan(paths: string[], recursive: boolean): Promise<MeshScanResult> {
  return invoke('mesh_scan', { paths, recursive })
}

export function meshPreviewPack(tabId: string, models: string[], options: PackOptionsJson): Promise<PreviewPayload> {
  return invoke('mesh_preview_pack', { tabId, models, options })
}

export function meshPack(tabId: string, models: string[], options: PackOptionsJson, outputDir: string, baseName: string): Promise<string> {
  return invoke('mesh_pack', { tabId, models, options, outputDir, baseName })
}
