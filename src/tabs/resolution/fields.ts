import { defineFields } from '@/components/ParamForm'
import type { Params } from '@/stores/session'

import { filterOptions, MAX_SIDE, roundOptions } from '../_imageOps/options'
import { RESOLUTION_METHODS, RESOLUTION_TARGETS } from './schema'

const method = (p: Params) => p.method as string
/** Anchor and pad color matter for pad/crop, and for resample + keep aspect (letterbox). */
const usesCanvas = (p: Params) => method(p) !== 'resample' || p.keepAspect === true

export const resolutionFields = defineFields([
  {
    kind: 'group',
    id: 'target',
    labelKey: 'resolution:groups.target',
    collapsible: false,
    fields: [
      {
        kind: 'segmented',
        key: 'target',
        labelKey: 'resolution:params.target.label',
        descKey: 'resolution:params.target.desc',
        options: RESOLUTION_TARGETS.map((value) => ({ value, labelKey: `resolution:params.target.options.${value}` })),
      },
      {
        kind: 'number',
        key: 'n',
        labelKey: 'resolution:params.n.label',
        descKey: 'resolution:params.n.desc',
        min: 1,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
        visibleIf: (p) => p.target === 'multipleOfN',
      },
      {
        kind: 'switch',
        key: 'allowNonSquare',
        labelKey: 'resolution:params.allowNonSquare.label',
        descKey: 'resolution:params.allowNonSquare.desc',
        visibleIf: (p) => p.target === 'pot',
      },
      {
        kind: 'number',
        key: 'maxSize',
        labelKey: 'resolution:params.maxSize.label',
        descKey: 'resolution:params.maxSize.desc',
        min: 0,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
      },
    ],
  },
  {
    kind: 'group',
    id: 'method',
    labelKey: 'resolution:groups.method',
    collapsible: false,
    fields: [
      {
        kind: 'segmented',
        key: 'method',
        labelKey: 'resolution:params.method.label',
        descKey: 'resolution:params.method.desc',
        options: RESOLUTION_METHODS.map((value) => ({ value, labelKey: `resolution:params.method.options.${value}` })),
      },
      {
        kind: 'segmented',
        key: 'round',
        labelKey: 'resolution:params.round.label',
        descKey: 'resolution:params.round.desc',
        options: roundOptions,
        // Pad always rounds up and crop always rounds down.
        disabledIf: (p) => method(p) !== 'resample',
      },
      {
        kind: 'select',
        key: 'filter',
        labelKey: 'resolution:params.filter.label',
        descKey: 'resolution:params.filter.desc',
        options: filterOptions,
        visibleIf: (p) => method(p) === 'resample',
      },
      {
        kind: 'switch',
        key: 'keepAspect',
        labelKey: 'resolution:params.keepAspect.label',
        descKey: 'resolution:params.keepAspect.desc',
        visibleIf: (p) => method(p) === 'resample',
      },
      {
        kind: 'anchor',
        key: 'anchor',
        labelKey: 'resolution:params.anchor.label',
        descKey: 'resolution:params.anchor.desc',
        visibleIf: usesCanvas,
      },
      {
        kind: 'color',
        key: 'padColor',
        labelKey: 'resolution:params.padColor.label',
        descKey: 'resolution:params.padColor.desc',
        alpha: true,
        // Crop never adds pixels.
        visibleIf: (p) => usesCanvas(p) && method(p) !== 'crop',
      },
    ],
  },
])
