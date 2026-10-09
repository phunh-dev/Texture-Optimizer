import { defineFields } from '@/components/ParamForm'

import { MAX_SIDE, snapOptions } from '../_imageOps/options'
import { TRIM_EMPTY_BEHAVIORS } from './schema'

export const trimFields = defineFields([
  {
    kind: 'group',
    id: 'detection',
    labelKey: 'trim:groups.detection',
    collapsible: false,
    fields: [
      {
        kind: 'slider',
        key: 'alphaThreshold',
        labelKey: 'trim:params.alphaThreshold.label',
        descKey: 'trim:params.alphaThreshold.desc',
        min: 0,
        max: 255,
      },
      {
        kind: 'number',
        key: 'margin',
        labelKey: 'trim:params.margin.label',
        descKey: 'trim:params.margin.desc',
        min: 0,
        max: MAX_SIDE,
        unitKey: 'common:units.px',
      },
      {
        kind: 'segmented',
        key: 'emptyBehavior',
        labelKey: 'trim:params.emptyBehavior.label',
        descKey: 'trim:params.emptyBehavior.desc',
        options: TRIM_EMPTY_BEHAVIORS.map((value) => ({ value, labelKey: `trim:params.emptyBehavior.options.${value}` })),
      },
    ],
  },
  {
    kind: 'group',
    id: 'sides',
    labelKey: 'trim:groups.sides',
    descKey: 'trim:groups.sidesDesc',
    fields: [
      { kind: 'switch', key: 'trimLeft', labelKey: 'trim:params.trimLeft.label' },
      { kind: 'switch', key: 'trimRight', labelKey: 'trim:params.trimRight.label' },
      { kind: 'switch', key: 'trimTop', labelKey: 'trim:params.trimTop.label' },
      { kind: 'switch', key: 'trimBottom', labelKey: 'trim:params.trimBottom.label' },
    ],
  },
  {
    kind: 'group',
    id: 'result',
    labelKey: 'trim:groups.result',
    collapsible: false,
    fields: [
      { kind: 'segmented', key: 'snap', labelKey: 'trim:params.snap.label', descKey: 'trim:params.snap.desc', options: snapOptions },
      {
        kind: 'switch',
        key: 'writeOffsets',
        labelKey: 'trim:params.writeOffsets.label',
        descKey: 'trim:params.writeOffsets.desc',
      },
    ],
  },
])
