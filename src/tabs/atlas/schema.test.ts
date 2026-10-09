import { beforeEach, describe, expect, it } from 'vitest'

import { validateParams, type FieldDescriptor, type GroupField, type SelectField } from '@/components/ParamForm'
import i18n from '@/i18n'
import { tKey } from '@/i18n/loose'

import { atlasFields } from './fields'
import { countStatuses, decodeAtlasPreview, type AtlasPreviewHeader } from './ipc'
import rustDefaults from './rustDefaults.json'
import {
  atlasDefaults,
  atlasSchema,
  baseNameProblem,
  buildAtlasRequest,
  EXPORTERS,
  heuristicsFor,
  MAXRECTS_HEURISTICS,
  projectFilePath,
  schemaParamDefaults,
  SKYLINE_HEURISTICS,
  supportsRotation,
} from './schema'
import { translateAtlasWarning } from './warnings'

function flatten(fields: FieldDescriptor[]): Exclude<FieldDescriptor, GroupField>[] {
  return fields.flatMap((f) => (f.kind === 'group' ? flatten(f.fields) : [f]))
}

describe('atlas schema', () => {
  beforeEach(async () => {
    await i18n.changeLanguage('en')
  })

  it('defaults equal the Rust defaults (shared fixture checked by cargo test too)', () => {
    const req = buildAtlasRequest(atlasDefaults())
    expect(req.params).toEqual(rustDefaults.params)
    expect(req.incrementalOptions).toEqual(rustDefaults.incremental)
    for (const kind of EXPORTERS) {
      expect(buildAtlasRequest({ ...atlasDefaults(), exporter: kind }).exporter).toEqual({
        kind,
        options: rustDefaults.exporters[kind],
      })
    }
    expect(req.baseName).toBe('atlas')
    expect(req.outputDir).toBe('')
    expect(validateParams(atlasSchema, atlasDefaults())).toEqual({})
    // The plain defaults used by the registry equal the zod schema defaults.
    expect(atlasDefaults()).toEqual(schemaParamDefaults())
  })

  it('every AtlasParams field has a control', () => {
    const keys = flatten(atlasFields('genericJson')).map((f) => f.key)
    for (const k of Object.keys(rustDefaults.params)) {
      if (k === 'heuristic') expect(keys).toEqual(expect.arrayContaining(['maxRectsHeuristic', 'skylineHeuristic']))
      else expect(keys).toContain(k)
    }
  })

  it('heuristic options are filtered by algorithm', () => {
    expect(heuristicsFor('maxRects')).toEqual(MAXRECTS_HEURISTICS)
    expect(heuristicsFor('skyline')).toEqual(SKYLINE_HEURISTICS)
    const fields = flatten(atlasFields('genericJson'))
    const maxRects = fields.find((f) => f.key === 'maxRectsHeuristic') as SelectField
    const skyline = fields.find((f) => f.key === 'skylineHeuristic') as SelectField
    expect(maxRects.options.map((o) => o.value)).toEqual([...MAXRECTS_HEURISTICS])
    expect(skyline.options.map((o) => o.value)).toEqual([...SKYLINE_HEURISTICS])
    expect(maxRects.visibleIf?.({ algorithm: 'maxRects' })).toBe(true)
    expect(maxRects.visibleIf?.({ algorithm: 'skyline' })).toBe(false)
    expect(skyline.visibleIf?.({ algorithm: 'skyline' })).toBe(true)
    expect(skyline.visibleIf?.({ algorithm: 'maxRects' })).toBe(false)
    // The request carries the heuristic of the selected algorithm only.
    const base = { ...atlasDefaults(), maxRectsHeuristic: 'contactPoint', skylineHeuristic: 'minWaste' }
    expect(buildAtlasRequest(base).params.heuristic).toBe('contactPoint')
    expect(buildAtlasRequest({ ...base, algorithm: 'skyline' }).params).toMatchObject({ algorithm: 'skyline', heuristic: 'minWaste' })
  })

  it('builds the exact request JSON', () => {
    const params = {
      ...atlasDefaults(),
      algorithm: 'skyline',
      skylineHeuristic: 'minWaste',
      maxWidth: 1024,
      maxHeight: 512,
      forcePot: false,
      forceSquare: true,
      sizeMode: 'fixed',
      padding: 4,
      extrude: 1,
      border: 3,
      trim: false,
      trimThreshold: 10,
      allowRotation: true,
      dedupe: false,
      sortBy: 'name',
      multiPage: true,
      premultiplyAlpha: true,
      exporter: 'unity',
      'unity.unityVersion': 'unity6',
      'unity.pixelsPerUnit': 32,
      'unity.filterMode': 'point',
      'unity.textureCompression': 'none',
      'unity.maxTextureSize': '4096',
      'unity.mipmaps': true,
      'unity.pivot': 'custom',
      'unity.customPivotX': 0.25,
      'unity.customPivotY': 0,
      'unity.preservePivotOnTrim': false,
      outputDir: '  D:/game/Assets/Atlases ',
      baseName: 'ui_icons',
      incrementalMode: 'repackOptimal',
      removeMissing: true,
    }
    expect(buildAtlasRequest(params)).toEqual({
      params: {
        algorithm: 'skyline',
        heuristic: 'minWaste',
        maxWidth: 1024,
        maxHeight: 512,
        forcePot: false,
        forceSquare: true,
        padding: 4,
        extrude: 1,
        border: 3,
        allowRotation: true,
        trim: false,
        trimThreshold: 10,
        dedupe: false,
        multiPage: true,
        sortBy: 'name',
        premultiplyAlpha: true,
        sizeMode: 'fixed',
      },
      exporter: {
        kind: 'unity',
        options: {
          unityVersion: 'unity6',
          pixelsPerUnit: 32,
          filterMode: 'point',
          textureCompression: 'none',
          maxTextureSize: 4096,
          mipmaps: true,
          pivot: 'custom',
          customPivot: { x: 0.25, y: 0 },
          preservePivotOnTrim: false,
        },
      },
      incrementalOptions: { mode: 'repackOptimal', removeMissing: true },
      outputDir: 'D:/game/Assets/Atlases',
      baseName: 'ui_icons',
    })
    expect(
      buildAtlasRequest({ ...params, exporter: 'godot', 'godot.version': 'godot3', 'godot.resPath': 'res://ui', 'godot.outputSubfolder': 'tres', 'godot.filterClip': true })
        .exporter,
    ).toEqual({ kind: 'godot', options: { version: 'godot3', resPath: 'res://ui', outputSubfolder: 'tres', filterClip: true } })
    expect(buildAtlasRequest({ ...params, exporter: 'unreal', 'unreal.pivotX': 0, 'unreal.pivotY': 1, 'unreal.fileExtension': 'json' }).exporter).toEqual({
      kind: 'unreal',
      options: { pivot: { x: 0, y: 1 }, fileExtension: 'json' },
    })
    expect(
      buildAtlasRequest({
        ...params,
        exporter: 'genericJson',
        'generic.format': 'array',
        'generic.imagePathPrefix': 'img/',
        'generic.includeTrimInfo': false,
        'generic.pretty': false,
        'generic.pivotX': 0,
        'generic.pivotY': 0.75,
      }).exporter,
    ).toEqual({
      kind: 'genericJson',
      options: { format: 'array', imagePathPrefix: 'img/', includeTrimInfo: false, pretty: false, pivot: { x: 0, y: 0.75 } },
    })
  })

  it('validates the base name like the Rust side', () => {
    expect(baseNameProblem('atlas')).toBeNull()
    expect(baseNameProblem('hero.sprites')).toBeNull()
    expect(baseNameProblem('')).toBe('empty')
    expect(baseNameProblem('  ')).toBe('empty')
    expect(baseNameProblem('a/b')).toBe('invalidChars')
    expect(baseNameProblem('a:b')).toBe('invalidChars')
    expect(baseNameProblem('a?')).toBe('invalidChars')
    expect(baseNameProblem('atlas.')).toBe('trailingDotOrSpace')
    expect(baseNameProblem('atlas ')).toBe('trailingDotOrSpace')
    expect(baseNameProblem('CON')).toBe('reserved')
    expect(baseNameProblem('lpt1.x')).toBe('reserved')
    expect(baseNameProblem('a'.repeat(121))).toBe('tooLong')
    const errors = validateParams(atlasSchema, { ...atlasDefaults(), baseName: 'bad|name' })
    expect(errors.baseName).toEqual(expect.objectContaining({ key: 'atlas:errors.baseName.invalidChars' }))
    expect(tKey(errors.baseName.key)).toBe('Base name cannot contain \\ / : * ? " < > |')
    expect(validateParams(atlasSchema, { ...atlasDefaults(), 'godot.resPath': 'C:/x' })['godot.resPath'].key).toBe('atlas:errors.resPath')
    expect(validateParams(atlasSchema, { ...atlasDefaults(), 'godot.outputSubfolder': '../up' })['godot.outputSubfolder'].key).toBe(
      'atlas:errors.subfolder',
    )
    expect(validateParams(atlasSchema, { ...atlasDefaults(), 'unity.pixelsPerUnit': 0 })['unity.pixelsPerUnit'].key).toBe('atlas:errors.positive')
  })

  it('rotation support mirrors ExporterConfig::supports_rotation', () => {
    expect(EXPORTERS.filter(supportsRotation)).toEqual(['genericJson', 'unreal'])
    const rot = (k: (typeof EXPORTERS)[number]) => flatten(atlasFields(k)).find((f) => f.key === 'allowRotation')!
    expect(rot('unity').disabledIf?.({})).toBe(true)
    expect(rot('unity').descKey).toBe('atlas:params.allowRotation.unsupported.unity')
    expect(rot('godot').descKey).toBe('atlas:params.allowRotation.unsupported.godot')
    expect(rot('unreal').disabledIf?.({})).toBe(false)
    expect(i18n.exists(rot('godot').descKey!)).toBe(true)
  })

  it('project file path keeps the folder separator style', () => {
    expect(projectFilePath('D:/out', 'atlas')).toBe('D:/out/atlas.texatlas.json')
    expect(projectFilePath('D:\\out\\', 'ui')).toBe('D:\\out\\ui.texatlas.json')
    expect(projectFilePath('/home/me/out/', 'a')).toBe('/home/me/out/a.texatlas.json')
  })
})

describe('atlas ipc helpers', () => {
  it('decodes the binary preview payload', () => {
    const png0 = new Uint8Array([1, 2, 3])
    const png1 = new Uint8Array([9, 8])
    const header: AtlasPreviewHeader = {
      pages: [
        { width: 64, height: 32, byteLength: png0.length },
        { width: 16, height: 16, byteLength: png1.length },
      ],
      frames: [],
      stats: { pages: [], spriteCount: 0, frameCount: 0, occupancy: 0 },
      warnings: [{ code: 'ATLAS_DUPLICATE_NAME', params: { name: 'a' } }],
      plan: [],
      hasPrevious: false,
      params: rustDefaults.params as AtlasPreviewHeader['params'],
    }
    const json = new TextEncoder().encode(JSON.stringify(header))
    const buf = new Uint8Array(4 + json.length + png0.length + png1.length)
    new DataView(buf.buffer).setUint32(0, json.length, true)
    buf.set(json, 4)
    buf.set(png0, 4 + json.length)
    buf.set(png1, 4 + json.length + png0.length)
    const out = decodeAtlasPreview(buf.buffer)
    expect(out.pages.map((p) => [p.width, p.height, [...new Uint8Array(p.png)]])).toEqual([
      [64, 32, [1, 2, 3]],
      [16, 16, [9, 8]],
    ])
    expect(out.warnings).toEqual(header.warnings)
    expect(() => decodeAtlasPreview(buf.slice(0, buf.length - 1).buffer)).toThrow()
  })

  it('counts plan statuses', () => {
    expect(
      countStatuses([
        { name: 'a', status: 'kept', sourcePath: null },
        { name: 'b', status: 'kept', sourcePath: null },
        { name: 'c', status: 'new', sourcePath: null },
      ]),
    ).toEqual({ kept: 2, new: 1, replaced: 0, removed: 0 })
  })

  it('translates warnings with localized exporter / feature names', async () => {
    await i18n.changeLanguage('en')
    expect(translateAtlasWarning({ code: 'ATLAS_FEATURE_DISABLED', params: { feature: 'rotation', exporter: 'unity' } })).toBe(
      'Rotation was turned off because Unity does not support it',
    )
    await i18n.changeLanguage('vi')
    expect(translateAtlasWarning({ code: 'ATLAS_FEATURE_DISABLED', params: { feature: 'rotation', exporter: 'godot' } })).toBe(
      'Đã tắt xoay sprite vì Godot không hỗ trợ',
    )
    await i18n.changeLanguage('en')
    expect(translateAtlasWarning({ code: 'ATLAS_LAYOUT_RESET', params: { reason: 'spacingChanged' } })).toBe(
      'Previous layout could not be kept (padding, extrude or border changed); atlas was repacked',
    )
  })
})
