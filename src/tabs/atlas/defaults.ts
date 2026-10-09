// Default atlas params as a plain object (no zod), so the eagerly loaded tool
// registry does not pull the schema into the main bundle. `schema.test.ts`
// checks these equal the zod schema defaults, which equal the Rust defaults.
import type { Params } from '@/stores/session'

const ATLAS_DEFAULTS = {
  algorithm: 'maxRects',
  maxRectsHeuristic: 'bestShortSideFit',
  skylineHeuristic: 'bottomLeft',
  maxWidth: 2048,
  maxHeight: 2048,
  forcePot: true,
  forceSquare: false,
  sizeMode: 'shrinkToFit',
  padding: 2,
  extrude: 0,
  border: 0,
  trim: true,
  trimThreshold: 0,
  allowRotation: false,
  dedupe: true,
  sortBy: 'area',
  multiPage: false,
  premultiplyAlpha: false,
  exporter: 'genericJson',
  'generic.format': 'hash',
  'generic.imagePathPrefix': '',
  'generic.includeTrimInfo': true,
  'generic.pretty': true,
  'generic.pivotX': 0.5,
  'generic.pivotY': 0.5,
  'unity.unityVersion': 'unity2022',
  'unity.pixelsPerUnit': 100,
  'unity.filterMode': 'bilinear',
  'unity.textureCompression': 'normalQuality',
  'unity.maxTextureSize': '0',
  'unity.mipmaps': false,
  'unity.pivot': 'center',
  'unity.customPivotX': 0.5,
  'unity.customPivotY': 0.5,
  'unity.preservePivotOnTrim': true,
  'godot.version': 'godot4',
  'godot.resPath': 'res://',
  'godot.outputSubfolder': '',
  'godot.filterClip': false,
  'unreal.pivotX': 0.5,
  'unreal.pivotY': 0.5,
  'unreal.fileExtension': 'paper2dsprites',
  outputDir: '',
  baseName: 'atlas',
  incrementalMode: 'keepPositions',
  removeMissing: false,
} as const

/** Fresh copy of the default params of a new atlas tab. */
export function atlasDefaults(): Params {
  return { ...ATLAS_DEFAULTS }
}
