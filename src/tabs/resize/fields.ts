import { defineFields } from '@/components/ParamForm'
import type { Params } from '@/stores/session'

import { filterOptions, MAX_SIDE, snapOptions } from '../_imageOps/options'
import { RESIZE_MODES } from './schema'

const mode = (p: Params) => p.mode as string

export const resizeFields = defineFields([
  {
    kind: 'group',
    id: 'size',
    labelKey: 'resize:groups.size',
    collapsible: false,
    fields: [
      {
        kind: 'select',
        key: 'mode',
        labelKey: 'resize:params.mode.label',
        descKey: 'resize:params.mode.desc',
        options: RESIZE_MODES.map((value) => ({ value, labelKey: `resize:params.mode.options.${value}` })),
      },
      {
        kind: 'number',
        key: 'percent',
        labelKey: 'resize:params.percent.label',
        descKey: 'resize:params.percent.desc',
        min: 0.01,
        max: 10000,
        step: 1,
        unitKey: 'common:units.percent',
        visibleIf: (p) => mode(p) === 'percent',
      },
      {
        kind: 'number',
        key: 'width',
        labelKey: 'resize:params.width.label',
        descKey: 'resize:params.width.desc',
        min: 1,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
        visibleIf: (p) => mode(p) === 'exact' || mode(p) === 'fitWidth',
      },
      {
        kind: 'number',
        key: 'height',
        labelKey: 'resize:params.height.label',
        descKey: 'resize:params.height.desc',
        min: 1,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
        visibleIf: (p) => mode(p) === 'exact' || mode(p) === 'fitHeight',
      },
      {
        kind: 'number',
        key: 'longestSide',
        labelKey: 'resize:params.longestSide.label',
        descKey: 'resize:params.longestSide.desc',
        min: 1,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
        visibleIf: (p) => mode(p) === 'longestSide',
      },
      {
        kind: 'switch',
        key: 'keepAspect',
        labelKey: 'resize:params.keepAspect.label',
        descKey: 'resize:params.keepAspect.desc',
        // Percent always keeps the aspect ratio.
        visibleIf: (p) => mode(p) !== 'percent',
      },
      {
        kind: 'segmented',
        key: 'snap',
        labelKey: 'resize:params.snap.label',
        descKey: 'resize:params.snap.desc',
        options: snapOptions,
      },
    ],
  },
  {
    kind: 'group',
    id: 'quality',
    labelKey: 'resize:groups.quality',
    fields: [
      { kind: 'select', key: 'filter', labelKey: 'resize:params.filter.label', descKey: 'resize:params.filter.desc', options: filterOptions },
      { kind: 'switch', key: 'linearSpace', labelKey: 'resize:params.linearSpace.label', descKey: 'resize:params.linearSpace.desc' },
      {
        kind: 'switch',
        key: 'premultiplyAlpha',
        labelKey: 'resize:params.premultiplyAlpha.label',
        descKey: 'resize:params.premultiplyAlpha.desc',
      },
    ],
  },
])
