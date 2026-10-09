import { describe, expect, it } from 'vitest'

import { schemaDefaults } from '@/components/ParamForm/validation'
import enErrors from '@/locales/en/errors.json'
import viErrors from '@/locales/vi/errors.json'
import { getTool } from '@/tabs/registry'

import { meshDefaults } from './defaults'
import type { MaterialInfo } from './ipc'
import { predictStatus, repeatTiles } from './preview'
import rustDefaults from './rustDefaults.json'
import { atlasFileName, buildPackOptions, fbxVerifyLocked, isValidBaseName, meshSchema, resolveParams } from './schema'

describe('meshPack schema', () => {
  it('plain defaults equal the zod defaults and the registry defaults', () => {
    expect(meshDefaults()).toEqual(schemaDefaults(meshSchema))
    expect(getTool('meshPack').defaultParams()).toEqual(meshDefaults())
    expect(meshSchema.safeParse(meshDefaults()).success).toBe(true)
  })

  it('the default request equals Rust PackOptions::default() (rustDefaults.json)', () => {
    expect(buildPackOptions(meshDefaults())).toEqual(rustDefaults)
  })

  it('builds the exact request JSON from flat params', () => {
    const req = buildPackOptions({
      ...meshDefaults(),
      channels: ['normal', 'baseColor', 'emissive'],
      maxSize: '1024',
      forceSquare: true,
      padding: 8,
      extrude: 2,
      textureScale: 50,
      scaleToFit: true,
      multiPage: false,
      inset: 'pixels',
      insetPixels: 1.5,
      outOfRange: 'bakeRepeat',
      maxTiles: 3,
      defaultNormal: '#7F7FFF',
      outputMode: 'uvRemapData',
      format: 'fbxAscii',
      mergeMaterials: false,
      mergedMaterialName: '  Shared  ',
      verifyGeometry: false,
      allowUnverifiedFbx: true,
      autoFallback: false,
      copySourceModels: false,
      outputDir: 'C:/out',
      baseName: 'props',
    })
    expect(req).toEqual({
      channels: ['baseColor', 'normal', 'emissive'],
      maxSize: 1024,
      forceSquare: true,
      padding: 8,
      extrude: 2,
      textureScale: 50,
      scaleToFit: true,
      multiPage: false,
      inset: { mode: 'pixels', pixels: 1.5 },
      outOfRange: { mode: 'bakeRepeat', maxTiles: 3 },
      missingDefaults: { ...rustDefaults.missingDefaults, normal: '#7f7fff' },
      output: {
        mode: 'uvRemapData',
        format: 'fbxAscii',
        mergeMaterials: false,
        mergedMaterialName: 'Shared',
        verifyGeometry: false,
        allowUnverifiedFbx: true,
        autoFallback: false,
        copySourceModels: false,
      },
    })
    // Output folder / base name travel as separate command args.
    expect(JSON.stringify(req)).not.toContain('C:/out')
    expect(buildPackOptions({ ...meshDefaults(), inset: 'none', outOfRange: 'clamp' })).toMatchObject({
      inset: { mode: 'none' },
      outOfRange: { mode: 'clamp' },
    })
  })

  it('falls back to defaults for invalid values instead of throwing', () => {
    const p = resolveParams({ padding: -3, maxSize: '3000', channels: ['nope'], format: 'gltf' })
    expect(p.padding).toBe(4)
    expect(p.maxSize).toBe('2048')
    expect(p.format).toBe('sameAsSource')
    expect(p.channels).toHaveLength(9)
  })

  it('requires a merged material name only when merging rewritten models', () => {
    const bad = meshSchema.safeParse({ ...meshDefaults(), mergedMaterialName: ' ' })
    expect(bad.success).toBe(false)
    expect(meshSchema.safeParse({ ...meshDefaults(), mergedMaterialName: ' ', mergeMaterials: false }).success).toBe(true)
  })

  it('FBX output locks the geometry check unless allowed', () => {
    expect(fbxVerifyLocked({ format: 'fbx' })).toBe(true)
    expect(fbxVerifyLocked({ format: 'fbxAscii' })).toBe(true)
    expect(fbxVerifyLocked({ format: 'fbx', allowUnverifiedFbx: true })).toBe(false)
    expect(fbxVerifyLocked({ format: 'obj' })).toBe(false)
  })

  it('base names and atlas file names follow the Rust rules', () => {
    expect(isValidBaseName('atlas')).toBe(true)
    expect(isValidBaseName('my atlas_2')).toBe(true)
    for (const bad of ['', ' a', 'a/b', 'a\\b', 'a:b', 'a.', 'a*']) expect(isValidBaseName(bad)).toBe(false)
    expect(atlasFileName('atlas', 'baseColor')).toBe('atlas_baseColor.png')
    expect(atlasFileName('a', 'other:x#1')).toBe('a_other-x-1.png')
  })
})

describe('UV status prediction', () => {
  const material = (range: MaterialInfo['uvRange'], textures = true): MaterialInfo => ({
    index: 0,
    name: 'M',
    meshCount: 1,
    vertexCount: 4,
    uvChannel: 0,
    uvRange: range,
    textures: textures
      ? [{ channel: 'baseColor', path: 'x.png', rawPath: 'x.png', exists: true, embedded: false, uvChannel: 0, width: 8, height: 8, mtimeMs: 1 }]
      : [],
  })
  const tiled = { min: [0, 0] as [number, number], max: [2, 1.00001] as [number, number], outOfRange: true }

  it('follows the out-of-range policy', () => {
    const d = meshDefaults()
    expect(predictStatus(material({ min: [0, 0], max: [1, 1], outOfRange: false }), d).status).toBe('inRange')
    expect(predictStatus(material(tiled), d).status).toBe('skipped')
    expect(predictStatus(material(tiled), { ...d, outOfRange: 'clamp' }).status).toBe('clamped')
    expect(predictStatus(material(tiled), { ...d, outOfRange: 'wrapIntoTile' }).status).toBe('wrapped')
    expect(predictStatus(material(tiled), { ...d, outOfRange: 'bakeRepeat', maxTiles: 2 })).toEqual({ status: 'repeated', tiles: [2, 1] })
    expect(predictStatus(material(tiled), { ...d, outOfRange: 'bakeRepeat', maxTiles: 1 }).status).toBe('tooManyTiles')
    expect(predictStatus(material(null), d).status).toBe('noUvs')
    expect(predictStatus(material(tiled, false), d).status).toBe('noTextures')
  })

  it('repeat tiles ignore float noise at tile borders', () => {
    expect(repeatTiles([0, 0], [1.00001, 1])).toEqual([1, 1])
    expect(repeatTiles([1.25, 0], [1.75, 1])).toEqual([1, 1])
    expect(repeatTiles([-1, 0], [2, 3])).toEqual([3, 3])
  })
})

describe('mesh error codes', () => {
  it('every code the packer can raise has en + vi messages', () => {
    const codes = [
      'MESH_IMPORT_FAILED',
      'MESH_EXPORT_FAILED',
      'MESH_FORMAT_UNSUPPORTED',
      'MESH_NO_UVS',
      'MESH_UV_OUT_OF_RANGE',
      'MESH_UV_TOO_MANY_TILES',
      'MESH_UV_WRAP_STRADDLE',
      'MESH_ATLAS_CHANNEL_MISSING',
      'MESH_TEXTURE_NOT_FOUND',
      'MESH_EXPORT_GEOMETRY_CHANGED',
      'MESH_BACKEND_UNAVAILABLE',
      'MESH_WORKER_CRASHED',
      'MESH_WORKER_SPAWN_FAILED',
      'MESH_NO_MODELS',
      'MESH_NOTHING_TO_PACK',
      'MESH_CHANNEL_FORCED',
      'MESH_TEXTURE_RESIZED',
      'MESH_TEXTURE_UNREADABLE',
      'MESH_MATERIAL_NO_TEXTURES',
      'MESH_MODEL_SPANS_PAGES',
      'MESH_FALLBACK_REMAP_DATA',
      'MESH_TEXTURES_DOWNSCALED',
      'MESH_MODEL_SKIPPED',
    ]
    for (const c of codes) {
      expect(enErrors, c).toHaveProperty(c)
      expect(viErrors, c).toHaveProperty(c)
    }
  })
})
