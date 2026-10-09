// Default params of the 3D Texture Packer as a plain object (no zod), so the
// eagerly loaded tool registry does not pull the schema into the main bundle.
// `schema.test.ts` checks these equal the zod defaults, and that the request
// built from them equals the Rust `PackOptions::default()` (rustDefaults.json).
import type { Params } from '@/stores/session'

export const CHANNELS = ['baseColor', 'normal', 'metallic', 'roughness', 'occlusion', 'emissive', 'opacity', 'specular', 'height'] as const
export type Channel = (typeof CHANNELS)[number]

/** Param key of the fill colour of a channel, e.g. `defaultNormal`. */
export function defaultColorKey(channel: Channel): string {
  return `default${channel[0].toUpperCase()}${channel.slice(1)}`
}

export const DEFAULT_COLORS: Record<Channel, string> = {
  baseColor: '#ffffff',
  normal: '#8080ff',
  metallic: '#000000',
  roughness: '#ffffff',
  occlusion: '#ffffff',
  emissive: '#000000',
  opacity: '#ffffff',
  specular: '#000000',
  height: '#808080',
}

export const MESH_DEFAULTS = {
  channels: [...CHANNELS] as string[],
  maxSize: '2048',
  forceSquare: false,
  padding: 4,
  extrude: 4,
  textureScale: 100,
  scaleToFit: false,
  multiPage: true,
  inset: 'halfTexel',
  insetPixels: 1,
  outOfRange: 'skipMaterial',
  maxTiles: 4,
  ...Object.fromEntries(CHANNELS.map((c) => [defaultColorKey(c), DEFAULT_COLORS[c]])),
  outputMode: 'rewriteModels',
  format: 'sameAsSource',
  mergeMaterials: true,
  mergedMaterialName: 'AtlasMaterial',
  verifyGeometry: true,
  allowUnverifiedFbx: false,
  autoFallback: true,
  copySourceModels: true,
  outputDir: '',
  baseName: 'atlas',
} as const satisfies Params

export function meshDefaults(): Params {
  return { ...MESH_DEFAULTS, channels: [...MESH_DEFAULTS.channels] }
}
