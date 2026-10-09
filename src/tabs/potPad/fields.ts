import { defineFields } from '@/components/ParamForm'

import { MAX_SIDE } from '../_imageOps/options'
import { PAD_FILLS, POT_TARGETS } from './schema'

export const potPadFields = defineFields([
  {
    kind: 'group',
    id: 'target',
    labelKey: 'potpad:groups.target',
    collapsible: false,
    fields: [
      {
        kind: 'segmented',
        key: 'target',
        labelKey: 'potpad:params.target.label',
        descKey: 'potpad:params.target.desc',
        options: POT_TARGETS.map((value) => ({ value, labelKey: `potpad:params.target.options.${value}` })),
      },
      {
        kind: 'number',
        key: 'width',
        labelKey: 'potpad:params.width.label',
        descKey: 'potpad:params.width.desc',
        min: 1,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
        visibleIf: (p) => p.target === 'fixed',
      },
      {
        kind: 'number',
        key: 'height',
        labelKey: 'potpad:params.height.label',
        descKey: 'potpad:params.height.desc',
        min: 1,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
        visibleIf: (p) => p.target === 'fixed',
      },
      {
        kind: 'number',
        key: 'minSize',
        labelKey: 'potpad:params.minSize.label',
        descKey: 'potpad:params.minSize.desc',
        min: 0,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
        // Ignored by the fixed target.
        visibleIf: (p) => p.target !== 'fixed',
      },
      {
        kind: 'number',
        key: 'maxSize',
        labelKey: 'potpad:params.maxSize.label',
        descKey: 'potpad:params.maxSize.desc',
        min: 0,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
      },
    ],
  },
  {
    kind: 'group',
    id: 'canvas',
    labelKey: 'potpad:groups.canvas',
    collapsible: false,
    fields: [
      { kind: 'anchor', key: 'anchor', labelKey: 'potpad:params.anchor.label', descKey: 'potpad:params.anchor.desc' },
      {
        kind: 'segmented',
        key: 'fill',
        labelKey: 'potpad:params.fill.label',
        descKey: 'potpad:params.fill.desc',
        options: PAD_FILLS.map((value) => ({ value, labelKey: `potpad:params.fill.options.${value}` })),
      },
      {
        kind: 'color',
        key: 'color',
        labelKey: 'potpad:params.color.label',
        descKey: 'potpad:params.color.desc',
        alpha: true,
        visibleIf: (p) => p.fill === 'color',
      },
    ],
  },
])
