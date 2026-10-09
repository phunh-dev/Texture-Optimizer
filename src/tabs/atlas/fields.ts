// ParamForm descriptors for the atlas panel. Built per render from the current
// exporter so unsupported features are greyed out with an explanation.
import { defineFields, type FieldDescriptor, type OptionDescriptor } from '@/components/ParamForm'
import type { Params } from '@/stores/session'

import {
  ALGORITHMS,
  EXPORTERS,
  GODOT_VERSIONS,
  JSON_FORMATS,
  MAX_ATLAS_SIZE,
  MAXRECTS_HEURISTICS,
  PAPER2D_EXTENSIONS,
  SIZE_MODES,
  SKYLINE_HEURISTICS,
  SORT_BY,
  supportsRotation,
  UNITY_COMPRESSIONS,
  UNITY_FILTERS,
  UNITY_MAX_SIZES,
  UNITY_PIVOTS,
  UNITY_VERSIONS,
  type ExporterKind,
} from './schema'

const options = (values: readonly string[], prefix: string): OptionDescriptor[] =>
  values.map((value) => ({ value, labelKey: `${prefix}.${value}` }))

const isExporter = (kind: ExporterKind) => (p: Params) => (p.exporter ?? 'genericJson') === kind
const PX = 'common:units.px'

function exporterFields(): FieldDescriptor[] {
  return [
    {
      kind: 'select',
      key: 'exporter',
      labelKey: 'atlas:exporter.kind.label',
      descKey: 'atlas:exporter.kind.desc',
      options: options(EXPORTERS, 'atlas:exporter.kinds'),
    },
    // Generic JSON
    {
      kind: 'segmented',
      key: 'generic.format',
      labelKey: 'atlas:exporter.generic.format.label',
      descKey: 'atlas:exporter.generic.format.desc',
      options: options(JSON_FORMATS, 'atlas:exporter.generic.formats'),
      visibleIf: isExporter('genericJson'),
    },
    {
      kind: 'text',
      key: 'generic.imagePathPrefix',
      labelKey: 'atlas:exporter.generic.imagePathPrefix.label',
      descKey: 'atlas:exporter.generic.imagePathPrefix.desc',
      placeholderKey: 'atlas:exporter.generic.imagePathPrefix.placeholder',
      monospace: true,
      visibleIf: isExporter('genericJson'),
    },
    {
      kind: 'switch',
      key: 'generic.includeTrimInfo',
      labelKey: 'atlas:exporter.generic.includeTrimInfo.label',
      descKey: 'atlas:exporter.generic.includeTrimInfo.desc',
      visibleIf: isExporter('genericJson'),
    },
    { kind: 'switch', key: 'generic.pretty', labelKey: 'atlas:exporter.generic.pretty.label', visibleIf: isExporter('genericJson') },
    {
      kind: 'number',
      key: 'generic.pivotX',
      labelKey: 'atlas:exporter.pivotX.label',
      descKey: 'atlas:exporter.pivotTopLeft',
      step: 0.05,
      visibleIf: isExporter('genericJson'),
    },
    { kind: 'number', key: 'generic.pivotY', labelKey: 'atlas:exporter.pivotY.label', step: 0.05, visibleIf: isExporter('genericJson') },
    // Unity
    {
      kind: 'select',
      key: 'unity.unityVersion',
      labelKey: 'atlas:exporter.unity.unityVersion.label',
      options: options(UNITY_VERSIONS, 'atlas:exporter.unity.versions'),
      visibleIf: isExporter('unity'),
    },
    {
      kind: 'number',
      key: 'unity.pixelsPerUnit',
      labelKey: 'atlas:exporter.unity.pixelsPerUnit.label',
      descKey: 'atlas:exporter.unity.pixelsPerUnit.desc',
      min: 0.01,
      step: 1,
      visibleIf: isExporter('unity'),
    },
    {
      kind: 'select',
      key: 'unity.filterMode',
      labelKey: 'atlas:exporter.unity.filterMode.label',
      options: options(UNITY_FILTERS, 'atlas:exporter.unity.filterModes'),
      visibleIf: isExporter('unity'),
    },
    {
      kind: 'select',
      key: 'unity.textureCompression',
      labelKey: 'atlas:exporter.unity.textureCompression.label',
      options: options(UNITY_COMPRESSIONS, 'atlas:exporter.unity.compressions'),
      visibleIf: isExporter('unity'),
    },
    {
      kind: 'select',
      key: 'unity.maxTextureSize',
      labelKey: 'atlas:exporter.unity.maxTextureSize.label',
      descKey: 'atlas:exporter.unity.maxTextureSize.desc',
      options: UNITY_MAX_SIZES.map((value) => ({
        value,
        labelKey: value === '0' ? 'atlas:exporter.unity.maxTextureSizes.auto' : `atlas:exporter.unity.maxTextureSizes.s${value}`,
      })),
      visibleIf: isExporter('unity'),
    },
    { kind: 'switch', key: 'unity.mipmaps', labelKey: 'atlas:exporter.unity.mipmaps.label', visibleIf: isExporter('unity') },
    {
      kind: 'select',
      key: 'unity.pivot',
      labelKey: 'atlas:exporter.unity.pivot.label',
      options: options(UNITY_PIVOTS, 'atlas:exporter.unity.pivots'),
      visibleIf: isExporter('unity'),
    },
    {
      kind: 'number',
      key: 'unity.customPivotX',
      labelKey: 'atlas:exporter.pivotX.label',
      descKey: 'atlas:exporter.pivotBottomLeft',
      step: 0.05,
      visibleIf: (p) => isExporter('unity')(p) && p['unity.pivot'] === 'custom',
    },
    {
      kind: 'number',
      key: 'unity.customPivotY',
      labelKey: 'atlas:exporter.pivotY.label',
      step: 0.05,
      visibleIf: (p) => isExporter('unity')(p) && p['unity.pivot'] === 'custom',
    },
    {
      kind: 'switch',
      key: 'unity.preservePivotOnTrim',
      labelKey: 'atlas:exporter.unity.preservePivotOnTrim.label',
      descKey: 'atlas:exporter.unity.preservePivotOnTrim.desc',
      visibleIf: isExporter('unity'),
    },
    // Godot
    {
      kind: 'segmented',
      key: 'godot.version',
      labelKey: 'atlas:exporter.godot.version.label',
      options: options(GODOT_VERSIONS, 'atlas:exporter.godot.versions'),
      visibleIf: isExporter('godot'),
    },
    {
      kind: 'text',
      key: 'godot.resPath',
      labelKey: 'atlas:exporter.godot.resPath.label',
      descKey: 'atlas:exporter.godot.resPath.desc',
      monospace: true,
      visibleIf: isExporter('godot'),
    },
    {
      kind: 'text',
      key: 'godot.outputSubfolder',
      labelKey: 'atlas:exporter.godot.outputSubfolder.label',
      descKey: 'atlas:exporter.godot.outputSubfolder.desc',
      placeholderKey: 'atlas:exporter.godot.outputSubfolder.placeholder',
      monospace: true,
      visibleIf: isExporter('godot'),
    },
    {
      kind: 'switch',
      key: 'godot.filterClip',
      labelKey: 'atlas:exporter.godot.filterClip.label',
      descKey: 'atlas:exporter.godot.filterClip.desc',
      visibleIf: isExporter('godot'),
    },
    // Unreal Paper2D
    {
      kind: 'number',
      key: 'unreal.pivotX',
      labelKey: 'atlas:exporter.pivotX.label',
      descKey: 'atlas:exporter.pivotTopLeft',
      step: 0.05,
      visibleIf: isExporter('unreal'),
    },
    { kind: 'number', key: 'unreal.pivotY', labelKey: 'atlas:exporter.pivotY.label', step: 0.05, visibleIf: isExporter('unreal') },
    {
      kind: 'segmented',
      key: 'unreal.fileExtension',
      labelKey: 'atlas:exporter.unreal.fileExtension.label',
      descKey: 'atlas:exporter.unreal.fileExtension.desc',
      options: options(PAPER2D_EXTENSIONS, 'atlas:exporter.unreal.extensions'),
      visibleIf: isExporter('unreal'),
    },
  ]
}

/** Every AtlasParams field, grouped, plus the exporter section. */
export function atlasFields(exporter: ExporterKind): FieldDescriptor[] {
  const rotationOk = supportsRotation(exporter)
  return defineFields([
    {
      kind: 'group',
      id: 'packing',
      labelKey: 'atlas:groups.packing',
      fields: [
        {
          kind: 'segmented',
          key: 'algorithm',
          labelKey: 'atlas:params.algorithm.label',
          descKey: 'atlas:params.algorithm.desc',
          options: options(ALGORITHMS, 'atlas:params.algorithms'),
        },
        {
          kind: 'select',
          key: 'maxRectsHeuristic',
          labelKey: 'atlas:params.heuristic.label',
          descKey: 'atlas:params.heuristic.desc',
          options: options(MAXRECTS_HEURISTICS, 'atlas:params.heuristics'),
          visibleIf: (p) => p.algorithm !== 'skyline',
        },
        {
          kind: 'select',
          key: 'skylineHeuristic',
          labelKey: 'atlas:params.heuristic.label',
          descKey: 'atlas:params.heuristic.desc',
          options: options(SKYLINE_HEURISTICS, 'atlas:params.heuristics'),
          visibleIf: (p) => p.algorithm === 'skyline',
        },
      ],
    },
    {
      kind: 'group',
      id: 'size',
      labelKey: 'atlas:groups.size',
      fields: [
        { kind: 'number', key: 'maxWidth', labelKey: 'atlas:params.maxWidth.label', min: 1, max: MAX_ATLAS_SIZE, step: 1, unitKey: PX },
        { kind: 'number', key: 'maxHeight', labelKey: 'atlas:params.maxHeight.label', min: 1, max: MAX_ATLAS_SIZE, step: 1, unitKey: PX },
        { kind: 'switch', key: 'forcePot', labelKey: 'atlas:params.forcePot.label', descKey: 'atlas:params.forcePot.desc' },
        { kind: 'switch', key: 'forceSquare', labelKey: 'atlas:params.forceSquare.label' },
        {
          kind: 'segmented',
          key: 'sizeMode',
          labelKey: 'atlas:params.sizeMode.label',
          descKey: 'atlas:params.sizeMode.desc',
          options: options(SIZE_MODES, 'atlas:params.sizeModes'),
        },
      ],
    },
    {
      kind: 'group',
      id: 'spacing',
      labelKey: 'atlas:groups.spacing',
      fields: [
        { kind: 'number', key: 'padding', labelKey: 'atlas:params.padding.label', descKey: 'atlas:params.padding.desc', min: 0, max: 1024, unitKey: PX },
        { kind: 'number', key: 'extrude', labelKey: 'atlas:params.extrude.label', descKey: 'atlas:params.extrude.desc', min: 0, max: 256, unitKey: PX },
        { kind: 'number', key: 'border', labelKey: 'atlas:params.border.label', descKey: 'atlas:params.border.desc', min: 0, max: 1024, unitKey: PX },
      ],
    },
    {
      kind: 'group',
      id: 'sprites',
      labelKey: 'atlas:groups.sprites',
      fields: [
        { kind: 'switch', key: 'trim', labelKey: 'atlas:params.trim.label', descKey: 'atlas:params.trim.desc' },
        {
          kind: 'slider',
          key: 'trimThreshold',
          labelKey: 'atlas:params.trimThreshold.label',
          descKey: 'atlas:params.trimThreshold.desc',
          min: 0,
          max: 255,
          visibleIf: (p) => p.trim !== false,
        },
        {
          kind: 'switch',
          key: 'allowRotation',
          labelKey: 'atlas:params.allowRotation.label',
          descKey: rotationOk ? 'atlas:params.allowRotation.desc' : `atlas:params.allowRotation.unsupported.${exporter}`,
          disabledIf: () => !rotationOk,
        },
        { kind: 'switch', key: 'dedupe', labelKey: 'atlas:params.dedupe.label', descKey: 'atlas:params.dedupe.desc' },
        { kind: 'select', key: 'sortBy', labelKey: 'atlas:params.sortBy.label', options: options(SORT_BY, 'atlas:params.sortOptions') },
      ],
    },
    {
      kind: 'group',
      id: 'pages',
      labelKey: 'atlas:groups.pages',
      fields: [{ kind: 'switch', key: 'multiPage', labelKey: 'atlas:params.multiPage.label', descKey: 'atlas:params.multiPage.desc' }],
    },
    {
      kind: 'group',
      id: 'color',
      labelKey: 'atlas:groups.color',
      fields: [
        { kind: 'switch', key: 'premultiplyAlpha', labelKey: 'atlas:params.premultiplyAlpha.label', descKey: 'atlas:params.premultiplyAlpha.desc' },
      ],
    },
    { kind: 'group', id: 'exporter', labelKey: 'atlas:groups.exporter', fields: exporterFields() },
  ])
}
