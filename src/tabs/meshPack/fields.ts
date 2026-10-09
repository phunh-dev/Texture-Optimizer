import { defineFields, type FieldDescriptor } from '@/components/ParamForm'
import type { Params } from '@/stores/session'

import { CHANNELS, defaultColorKey } from './defaults'
import { fbxVerifyLocked, FORMATS, INSETS, MAX_SIZES, OUT_OF_RANGE, OUTPUT_MODES } from './schema'

const rewrite = (p: Params) => p.outputMode !== 'uvRemapData'

export const meshFields: FieldDescriptor[] = defineFields([
  {
    kind: 'group',
    id: 'channels',
    labelKey: 'mesh:groups.channels',
    fields: [
      {
        kind: 'multiSelect',
        key: 'channels',
        labelKey: 'mesh:params.channels.label',
        descKey: 'mesh:params.channels.desc',
        options: CHANNELS.map((c) => ({ value: c, labelKey: `mesh:channels.${c}` })),
      },
    ],
  },
  {
    kind: 'group',
    id: 'atlas',
    labelKey: 'mesh:groups.atlas',
    fields: [
      {
        kind: 'select',
        key: 'maxSize',
        labelKey: 'mesh:params.maxSize.label',
        descKey: 'mesh:params.maxSize.desc',
        options: MAX_SIZES.map((s) => ({ value: s, labelKey: `mesh:sizes.s${s}` })),
      },
      { kind: 'switch', key: 'forceSquare', labelKey: 'mesh:params.forceSquare.label' },
      { kind: 'slider', key: 'padding', labelKey: 'mesh:params.padding.label', descKey: 'mesh:params.padding.desc', min: 0, max: 32, unitKey: 'common:units.px' },
      { kind: 'slider', key: 'extrude', labelKey: 'mesh:params.extrude.label', descKey: 'mesh:params.extrude.desc', min: 0, max: 32, unitKey: 'common:units.px' },
      {
        kind: 'slider',
        key: 'textureScale',
        labelKey: 'mesh:params.textureScale.label',
        descKey: 'mesh:params.textureScale.desc',
        min: 5,
        max: 100,
        step: 5,
        unitKey: 'common:units.percent',
      },
      { kind: 'switch', key: 'scaleToFit', labelKey: 'mesh:params.scaleToFit.label', descKey: 'mesh:params.scaleToFit.desc' },
      { kind: 'switch', key: 'multiPage', labelKey: 'mesh:params.multiPage.label', descKey: 'mesh:params.multiPage.desc' },
    ],
  },
  {
    kind: 'group',
    id: 'uv',
    labelKey: 'mesh:groups.uv',
    fields: [
      {
        kind: 'segmented',
        key: 'inset',
        labelKey: 'mesh:params.inset.label',
        descKey: 'mesh:params.inset.desc',
        options: INSETS.map((v) => ({ value: v, labelKey: `mesh:params.inset.${v}` })),
      },
      {
        kind: 'number',
        key: 'insetPixels',
        labelKey: 'mesh:params.insetPixels.label',
        min: 0,
        max: 64,
        step: 0.5,
        unitKey: 'common:units.px',
        visibleIf: (p) => p.inset === 'pixels',
      },
      {
        kind: 'select',
        key: 'outOfRange',
        labelKey: 'mesh:params.outOfRange.label',
        descKey: 'mesh:params.outOfRange.desc',
        options: OUT_OF_RANGE.map((v) => ({ value: v, labelKey: `mesh:params.outOfRange.${v}` })),
      },
      {
        kind: 'number',
        key: 'maxTiles',
        labelKey: 'mesh:params.maxTiles.label',
        descKey: 'mesh:params.maxTiles.desc',
        min: 1,
        max: 64,
        step: 1,
        visibleIf: (p) => p.outOfRange === 'bakeRepeat',
      },
    ],
  },
  {
    kind: 'group',
    id: 'defaults',
    labelKey: 'mesh:groups.defaults',
    descKey: 'mesh:groups.defaultsDesc',
    defaultOpen: false,
    fields: CHANNELS.map((c) => ({ kind: 'color' as const, key: defaultColorKey(c), labelKey: `mesh:channels.${c}` })),
  },
  {
    kind: 'group',
    id: 'output',
    labelKey: 'mesh:groups.output',
    fields: [
      {
        kind: 'segmented',
        key: 'outputMode',
        labelKey: 'mesh:params.outputMode.label',
        options: OUTPUT_MODES.map((v) => ({ value: v, labelKey: `mesh:params.outputMode.${v}` })),
      },
      {
        kind: 'select',
        key: 'format',
        labelKey: 'mesh:params.format.label',
        descKey: 'mesh:params.outputMode.rewriteDesc',
        options: FORMATS.map((v) => ({ value: v, labelKey: `mesh:params.format.${v}` })),
        visibleIf: rewrite,
      },
      { kind: 'switch', key: 'mergeMaterials', labelKey: 'mesh:params.mergeMaterials.label', descKey: 'mesh:params.mergeMaterials.desc' },
      {
        kind: 'text',
        key: 'mergedMaterialName',
        labelKey: 'mesh:params.mergedMaterialName.label',
        visibleIf: (p) => p.mergeMaterials === true,
      },
      {
        kind: 'switch',
        key: 'verifyGeometry',
        labelKey: 'mesh:params.verifyGeometry.label',
        descKey: 'mesh:params.verifyGeometry.desc',
        visibleIf: rewrite,
        disabledIf: fbxVerifyLocked,
      },
      { kind: 'switch', key: 'autoFallback', labelKey: 'mesh:params.autoFallback.label', descKey: 'mesh:params.autoFallback.desc', visibleIf: rewrite },
      {
        kind: 'switch',
        key: 'copySourceModels',
        labelKey: 'mesh:params.copySourceModels.label',
        descKey: 'mesh:params.copySourceModels.desc',
        visibleIf: (p) => !rewrite(p) || p.autoFallback === true,
      },
    ],
  },
  {
    kind: 'group',
    id: 'advanced',
    labelKey: 'mesh:groups.advanced',
    defaultOpen: false,
    visibleIf: rewrite,
    fields: [{ kind: 'switch', key: 'allowUnverifiedFbx', labelKey: 'mesh:params.allowUnverifiedFbx.label', descKey: 'mesh:params.allowUnverifiedFbx.desc' }],
  },
])
