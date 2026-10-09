// 3D Texture Packer params. Stored FLAT in the session (ParamForm binds
// top-level keys); `buildPackOptions` maps them to the Rust `PackOptions`
// (crates/texopt-core/src/mesh/pack/options.rs). Defaults are checked against
// `rustDefaults.json` (serialized `PackOptions::default()`) on both sides.
import { z } from 'zod'

import type { Params } from '@/stores/session'

import { CHANNELS, DEFAULT_COLORS, defaultColorKey, MESH_DEFAULTS, type Channel } from './defaults'

export const MAX_SIZES = ['256', '512', '1024', '2048', '4096', '8192', '16384'] as const
export const INSETS = ['halfTexel', 'none', 'pixels'] as const
export const OUT_OF_RANGE = ['skipMaterial', 'clamp', 'wrapIntoTile', 'bakeRepeat'] as const
export const OUTPUT_MODES = ['rewriteModels', 'uvRemapData'] as const
export const FORMATS = ['sameAsSource', 'obj', 'collada', 'fbx', 'fbxAscii'] as const

export type OutputModeValue = (typeof OUTPUT_MODES)[number]
export type FormatValue = (typeof FORMATS)[number]

// eslint-disable-next-line no-control-regex
const INVALID_CHARS = /[\\/:*?"<>|\u0000-\u001f]/

/** Same rules as Rust `pack::validate_base_name`. */
export function isValidBaseName(name: string): boolean {
  return name.trim() !== '' && name === name.trim() && !name.endsWith('.') && !INVALID_CHARS.test(name)
}

const hex = z.string().regex(/^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$/)
const int = (min: number, max: number) => z.number().int().min(min).max(max)

export const meshSchema = z.object({
  channels: z.array(z.enum(CHANNELS)).default([...CHANNELS]),
  maxSize: z.enum(MAX_SIZES).default('2048'),
  forceSquare: z.boolean().default(false),
  padding: int(0, 64).default(4),
  extrude: int(0, 64).default(4),
  textureScale: int(1, 100).default(100),
  scaleToFit: z.boolean().default(false),
  multiPage: z.boolean().default(true),
  inset: z.enum(INSETS).default('halfTexel'),
  insetPixels: z.number().min(0).max(64).default(1),
  outOfRange: z.enum(OUT_OF_RANGE).default('skipMaterial'),
  maxTiles: int(1, 64).default(4),
  ...(Object.fromEntries(CHANNELS.map((c) => [defaultColorKey(c), hex.default(DEFAULT_COLORS[c])])) as Record<
    string,
    z.ZodDefault<typeof hex>
  >),
  outputMode: z.enum(OUTPUT_MODES).default('rewriteModels'),
  format: z.enum(FORMATS).default('sameAsSource'),
  mergeMaterials: z.boolean().default(true),
  mergedMaterialName: z.string().default('AtlasMaterial'),
  verifyGeometry: z.boolean().default(true),
  allowUnverifiedFbx: z.boolean().default(false),
  autoFallback: z.boolean().default(true),
  copySourceModels: z.boolean().default(true),
  outputDir: z.string().default(''),
  baseName: z.string().default('atlas'),
}).superRefine((p, ctx) => {
  if (p.mergeMaterials && p.outputMode === 'rewriteModels' && p.mergedMaterialName.trim() === '') {
    ctx.addIssue({ code: 'custom', path: ['mergedMaterialName'], message: 'mesh:errors.mergedNameRequired' })
  }
})

export type MeshParams = z.output<typeof meshSchema>

/** Params with defaults filled in for missing/invalid keys (never throws). */
export function resolveParams(params: Params): MeshParams {
  const merged = { ...MESH_DEFAULTS, ...params }
  const parsed = meshSchema.safeParse(merged)
  if (parsed.success) return parsed.data
  const fixed: Record<string, unknown> = { ...merged }
  for (const issue of parsed.error.issues) {
    const key = String(issue.path[0] ?? '')
    if (key in MESH_DEFAULTS) fixed[key] = (MESH_DEFAULTS as Record<string, unknown>)[key]
  }
  const again = meshSchema.safeParse(fixed)
  return again.success ? again.data : (meshSchema.parse(MESH_DEFAULTS) as MeshParams)
}

/** FBX output is always verified unless the advanced switch allows otherwise. */
export function fbxVerifyLocked(p: Params): boolean {
  return (p.format === 'fbx' || p.format === 'fbxAscii') && p.allowUnverifiedFbx !== true
}

// ------------------------------------------------------------- request

export type InsetJson = { mode: 'halfTexel' } | { mode: 'none' } | { mode: 'pixels'; pixels: number }
export type OutOfRangeJson =
  | { mode: 'skipMaterial' }
  | { mode: 'clamp' }
  | { mode: 'wrapIntoTile' }
  | { mode: 'bakeRepeat'; maxTiles: number }

/** Mirrors Rust `PackOptions` (camelCase). */
export interface PackOptionsJson {
  channels: Channel[]
  maxSize: number
  forceSquare: boolean
  padding: number
  extrude: number
  textureScale: number
  scaleToFit: boolean
  multiPage: boolean
  inset: InsetJson
  outOfRange: OutOfRangeJson
  missingDefaults: Record<Channel, string>
  output: {
    mode: OutputModeValue
    format: FormatValue
    mergeMaterials: boolean
    mergedMaterialName: string
    verifyGeometry: boolean
    allowUnverifiedFbx: boolean
    autoFallback: boolean
    copySourceModels: boolean
  }
}

export function buildPackOptions(params: Params): PackOptionsJson {
  const p = resolveParams(params)
  const inset: InsetJson = p.inset === 'pixels' ? { mode: 'pixels', pixels: p.insetPixels } : { mode: p.inset }
  const outOfRange: OutOfRangeJson = p.outOfRange === 'bakeRepeat' ? { mode: 'bakeRepeat', maxTiles: p.maxTiles } : { mode: p.outOfRange }
  const colors = p as unknown as Record<string, string>
  return {
    channels: CHANNELS.filter((c) => p.channels.includes(c)),
    maxSize: Number(p.maxSize),
    forceSquare: p.forceSquare,
    padding: p.padding,
    extrude: p.extrude,
    textureScale: p.textureScale,
    scaleToFit: p.scaleToFit,
    multiPage: p.multiPage,
    inset,
    outOfRange,
    missingDefaults: Object.fromEntries(CHANNELS.map((c) => [c, colors[defaultColorKey(c)].toLowerCase()])) as Record<Channel, string>,
    output: {
      mode: p.outputMode,
      format: p.format,
      mergeMaterials: p.mergeMaterials,
      mergedMaterialName: p.mergedMaterialName.trim() || 'AtlasMaterial',
      verifyGeometry: p.verifyGeometry,
      allowUnverifiedFbx: p.allowUnverifiedFbx,
      autoFallback: p.autoFallback,
      copySourceModels: p.copySourceModels,
    },
  }
}

/** Atlas file names (same rule as Rust `atlas_file_name`, page 1). */
export function atlasFileName(base: string, channel: string): string {
  return `${base}_${channel.replace(/[^A-Za-z0-9_-]/g, '-')}.png`
}
