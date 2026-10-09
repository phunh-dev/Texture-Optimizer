// Smart Atlas params. The session stores them FLAT (ParamForm binds top-level
// keys); exporter options are namespaced with a prefix ('unity.pixelsPerUnit').
// `buildAtlasRequest` maps them to the Rust shapes (AtlasParams,
// ExporterConfig, IncrementalOptions). Defaults must equal the Rust defaults:
// both sides are checked against `rustDefaults.json`.
import { z } from 'zod'

import { schemaDefaults } from '@/components/ParamForm/validation'
import type { Params } from '@/stores/session'

import { atlasDefaults } from './defaults'

export const ALGORITHMS = ['maxRects', 'skyline'] as const
export const MAXRECTS_HEURISTICS = ['bestShortSideFit', 'bestLongSideFit', 'bestAreaFit', 'bottomLeft', 'contactPoint'] as const
export const SKYLINE_HEURISTICS = ['bottomLeft', 'minWaste'] as const
export const SORT_BY = ['area', 'maxSide', 'height', 'width', 'name', 'none'] as const
export const SIZE_MODES = ['shrinkToFit', 'fixed'] as const
export const EXPORTERS = ['genericJson', 'unity', 'godot', 'unreal'] as const
export const JSON_FORMATS = ['hash', 'array'] as const
export const UNITY_VERSIONS = ['unity2021', 'unity2022', 'unity6'] as const
export const UNITY_FILTERS = ['point', 'bilinear', 'trilinear'] as const
export const UNITY_COMPRESSIONS = ['none', 'lowQuality', 'normalQuality', 'highQuality'] as const
/** '0' = automatic (smallest Unity size holding the page). Select values are strings. */
export const UNITY_MAX_SIZES = ['0', '32', '64', '128', '256', '512', '1024', '2048', '4096', '8192', '16384'] as const
export const UNITY_PIVOTS = [
  'center',
  'topLeft',
  'topCenter',
  'topRight',
  'leftCenter',
  'rightCenter',
  'bottomLeft',
  'bottomCenter',
  'bottomRight',
  'custom',
] as const
export const GODOT_VERSIONS = ['godot3', 'godot4'] as const
export const PAPER2D_EXTENSIONS = ['paper2dsprites', 'json'] as const
export const INCREMENTAL_MODES = ['keepPositions', 'repackOptimal'] as const

export type Algorithm = (typeof ALGORITHMS)[number]
export type ExporterKind = (typeof EXPORTERS)[number]
export type IncrementalMode = (typeof INCREMENTAL_MODES)[number]

/** Max page edge accepted by the packer (Rust MAX_ATLAS_SIZE). */
export const MAX_ATLAS_SIZE = 16384

export function heuristicsFor(algorithm: Algorithm): readonly string[] {
  return algorithm === 'skyline' ? SKYLINE_HEURISTICS : MAXRECTS_HEURISTICS
}

/** Mirrors ExporterConfig::supports_rotation: Unity sprite rects and Godot regions cannot rotate. */
export function supportsRotation(kind: ExporterKind): boolean {
  return kind === 'genericJson' || kind === 'unreal'
}

const RESERVED = /^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/i
// eslint-disable-next-line no-control-regex
const INVALID_CHARS = /[\\/:*?"<>|\u0000-\u001f\u007f]/
export const MAX_BASE_NAME_LEN = 120

export type BaseNameProblem = 'empty' | 'tooLong' | 'invalidChars' | 'trailingDotOrSpace' | 'reserved'

/** Same rules as Rust `validate_base_name` (valid file name on Windows/macOS/Linux). */
export function baseNameProblem(name: string): BaseNameProblem | null {
  if (name.trim() === '') return 'empty'
  if ([...name].length > MAX_BASE_NAME_LEN) return 'tooLong'
  if (INVALID_CHARS.test(name)) return 'invalidChars'
  if (name.endsWith('.') || name.endsWith(' ') || name.startsWith(' ')) return 'trailingDotOrSpace'
  if (RESERVED.test(name.split('.')[0])) return 'reserved'
  return null
}

const int = (min: number, max: number) => z.number().int().min(min).max(max)

export const atlasSchema = z.object({
  // Packing
  algorithm: z.enum(ALGORITHMS).default('maxRects'),
  maxRectsHeuristic: z.enum(MAXRECTS_HEURISTICS).default('bestShortSideFit'),
  skylineHeuristic: z.enum(SKYLINE_HEURISTICS).default('bottomLeft'),
  // Size
  maxWidth: int(1, MAX_ATLAS_SIZE).default(2048),
  maxHeight: int(1, MAX_ATLAS_SIZE).default(2048),
  forcePot: z.boolean().default(true),
  forceSquare: z.boolean().default(false),
  sizeMode: z.enum(SIZE_MODES).default('shrinkToFit'),
  // Spacing
  padding: int(0, 1024).default(2),
  extrude: int(0, 256).default(0),
  border: int(0, 1024).default(0),
  // Sprites
  trim: z.boolean().default(true),
  trimThreshold: int(0, 255).default(0),
  allowRotation: z.boolean().default(false),
  dedupe: z.boolean().default(true),
  sortBy: z.enum(SORT_BY).default('area'),
  // Pages / color
  multiPage: z.boolean().default(false),
  premultiplyAlpha: z.boolean().default(false),
  // Exporter
  exporter: z.enum(EXPORTERS).default('genericJson'),
  'generic.format': z.enum(JSON_FORMATS).default('hash'),
  'generic.imagePathPrefix': z.string().default(''),
  'generic.includeTrimInfo': z.boolean().default(true),
  'generic.pretty': z.boolean().default(true),
  'generic.pivotX': z.number().default(0.5),
  'generic.pivotY': z.number().default(0.5),
  'unity.unityVersion': z.enum(UNITY_VERSIONS).default('unity2022'),
  'unity.pixelsPerUnit': z.number().gt(0, { error: 'atlas:errors.positive' }).max(100000).default(100),
  'unity.filterMode': z.enum(UNITY_FILTERS).default('bilinear'),
  'unity.textureCompression': z.enum(UNITY_COMPRESSIONS).default('normalQuality'),
  'unity.maxTextureSize': z.enum(UNITY_MAX_SIZES).default('0'),
  'unity.mipmaps': z.boolean().default(false),
  'unity.pivot': z.enum(UNITY_PIVOTS).default('center'),
  'unity.customPivotX': z.number().default(0.5),
  'unity.customPivotY': z.number().default(0.5),
  'unity.preservePivotOnTrim': z.boolean().default(true),
  'godot.version': z.enum(GODOT_VERSIONS).default('godot4'),
  'godot.resPath': z.string().startsWith('res://', { error: 'atlas:errors.resPath' }).default('res://'),
  'godot.outputSubfolder': z
    .string()
    .refine((s) => !s.includes(':') && !s.replace(/\\/g, '/').split('/').includes('..'), { error: 'atlas:errors.subfolder' })
    .default(''),
  'godot.filterClip': z.boolean().default(false),
  'unreal.pivotX': z.number().default(0.5),
  'unreal.pivotY': z.number().default(0.5),
  'unreal.fileExtension': z.enum(PAPER2D_EXTENSIONS).default('paper2dsprites'),
  // Output (own panel)
  outputDir: z.string().default(''),
  baseName: z
    .string()
    .superRefine((name, ctx) => {
      const problem = baseNameProblem(name)
      if (problem) ctx.addIssue({ code: 'custom', message: `atlas:errors.baseName.${problem}` })
    })
    .default('atlas'),
  incrementalMode: z.enum(INCREMENTAL_MODES).default('keepPositions'),
  removeMissing: z.boolean().default(false),
})

export type AtlasFormParams = z.infer<typeof atlasSchema>

export { atlasDefaults }

/** Defaults derived from the zod schema (tests check they equal `atlasDefaults()`). */
export function schemaParamDefaults(): Params {
  return schemaDefaults(atlasSchema)
}

// ---------------------------------------------------------------------------
// Request shapes (mirror the Rust serde structs)

export interface AtlasParamsJson {
  algorithm: Algorithm
  heuristic: string
  maxWidth: number
  maxHeight: number
  forcePot: boolean
  forceSquare: boolean
  padding: number
  extrude: number
  border: number
  allowRotation: boolean
  trim: boolean
  trimThreshold: number
  dedupe: boolean
  multiPage: boolean
  sortBy: string
  premultiplyAlpha: boolean
  sizeMode: string
}

export interface PivotJson {
  x: number
  y: number
}

export type ExporterConfigJson =
  | {
      kind: 'genericJson'
      options: { format: string; imagePathPrefix: string; includeTrimInfo: boolean; pretty: boolean; pivot: PivotJson }
    }
  | {
      kind: 'unity'
      options: {
        unityVersion: string
        pixelsPerUnit: number
        filterMode: string
        textureCompression: string
        maxTextureSize: number
        mipmaps: boolean
        pivot: string
        customPivot: PivotJson
        preservePivotOnTrim: boolean
      }
    }
  | { kind: 'godot'; options: { version: string; resPath: string; outputSubfolder: string; filterClip: boolean } }
  | { kind: 'unreal'; options: { pivot: PivotJson; fileExtension: string } }

export interface IncrementalOptionsJson {
  mode: IncrementalMode
  removeMissing: boolean
}

export interface AtlasRequest {
  params: AtlasParamsJson
  exporter: ExporterConfigJson
  incrementalOptions: IncrementalOptionsJson
  outputDir: string
  baseName: string
}

/** Session params (missing keys fall back to defaults) with their inferred types. */
export function resolveParams(params: Params): AtlasFormParams {
  return { ...(atlasDefaults() as AtlasFormParams), ...(params as Partial<AtlasFormParams>) }
}

export function exporterConfig(p: AtlasFormParams): ExporterConfigJson {
  switch (p.exporter) {
    case 'unity':
      return {
        kind: 'unity',
        options: {
          unityVersion: p['unity.unityVersion'],
          pixelsPerUnit: p['unity.pixelsPerUnit'],
          filterMode: p['unity.filterMode'],
          textureCompression: p['unity.textureCompression'],
          maxTextureSize: Number(p['unity.maxTextureSize']),
          mipmaps: p['unity.mipmaps'],
          pivot: p['unity.pivot'],
          customPivot: { x: p['unity.customPivotX'], y: p['unity.customPivotY'] },
          preservePivotOnTrim: p['unity.preservePivotOnTrim'],
        },
      }
    case 'godot':
      return {
        kind: 'godot',
        options: {
          version: p['godot.version'],
          resPath: p['godot.resPath'],
          outputSubfolder: p['godot.outputSubfolder'],
          filterClip: p['godot.filterClip'],
        },
      }
    case 'unreal':
      return {
        kind: 'unreal',
        options: { pivot: { x: p['unreal.pivotX'], y: p['unreal.pivotY'] }, fileExtension: p['unreal.fileExtension'] },
      }
    default:
      return {
        kind: 'genericJson',
        options: {
          format: p['generic.format'],
          imagePathPrefix: p['generic.imagePathPrefix'],
          includeTrimInfo: p['generic.includeTrimInfo'],
          pretty: p['generic.pretty'],
          pivot: { x: p['generic.pivotX'], y: p['generic.pivotY'] },
        },
      }
  }
}

/** Maps the flat session params to the command arguments. */
export function buildAtlasRequest(params: Params): AtlasRequest {
  const p = resolveParams(params)
  return {
    params: {
      algorithm: p.algorithm,
      heuristic: p.algorithm === 'skyline' ? p.skylineHeuristic : p.maxRectsHeuristic,
      maxWidth: p.maxWidth,
      maxHeight: p.maxHeight,
      forcePot: p.forcePot,
      forceSquare: p.forceSquare,
      padding: p.padding,
      extrude: p.extrude,
      border: p.border,
      allowRotation: p.allowRotation,
      trim: p.trim,
      trimThreshold: p.trimThreshold,
      dedupe: p.dedupe,
      multiPage: p.multiPage,
      sortBy: p.sortBy,
      premultiplyAlpha: p.premultiplyAlpha,
      sizeMode: p.sizeMode,
    },
    exporter: exporterConfig(p),
    incrementalOptions: { mode: p.incrementalMode, removeMissing: p.removeMissing },
    outputDir: p.outputDir.trim(),
    baseName: p.baseName,
  }
}

/** `<dir>/<base>.texatlas.json` (keeps the folder's own separator style). */
export function projectFilePath(dir: string, base: string): string {
  const d = dir.trim()
  const sep = d.includes('\\') && !d.includes('/') ? '\\' : '/'
  return `${d.replace(/[\\/]+$/, '')}${sep}${base}.texatlas.json`
}
