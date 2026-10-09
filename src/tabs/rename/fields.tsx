import { defineFields } from '@/components/ParamForm'

import { FolderControl, RulesControl, SmartControl, TemplateControl } from './controls'
import { CASE_MODES, EXTENSION_CASES, MAX_ZERO_PAD, SORT_BY } from './schema'

export const renameFields = defineFields([
  {
    kind: 'group',
    id: 'rename.name',
    labelKey: 'rename:groups.name',
    fields: [
      {
        kind: 'custom',
        key: 'template',
        labelKey: 'rename:params.template.label',
        descKey: 'rename:params.template.desc',
        render: (props) => <TemplateControl {...props} />,
      },
      { kind: 'text', key: 'prefix', labelKey: 'rename:params.prefix.label', monospace: true },
      { kind: 'text', key: 'suffix', labelKey: 'rename:params.suffix.label', monospace: true },
      {
        kind: 'text',
        key: 'dateFormat',
        labelKey: 'rename:params.dateFormat.label',
        descKey: 'rename:params.dateFormat.desc',
        monospace: true,
        visibleIf: (p) => typeof p.template === 'string' && p.template.includes('{date}'),
      },
      {
        kind: 'select',
        key: 'case',
        labelKey: 'rename:params.case.label',
        options: CASE_MODES.map((c) => ({ value: c, labelKey: `rename:params.case.${c}` })),
      },
    ],
  },
  {
    kind: 'group',
    id: 'rename.numbering',
    labelKey: 'rename:groups.numbering',
    descKey: 'rename:groups.numberingDesc',
    fields: [
      { kind: 'number', key: 'startNumber', labelKey: 'rename:params.startNumber.label', step: 1 },
      { kind: 'number', key: 'step', labelKey: 'rename:params.step.label', step: 1 },
      { kind: 'number', key: 'zeroPad', labelKey: 'rename:params.zeroPad.label', descKey: 'rename:params.zeroPad.desc', min: 0, max: MAX_ZERO_PAD, step: 1 },
      {
        kind: 'select',
        key: 'sortBy',
        labelKey: 'rename:params.sortBy.label',
        options: SORT_BY.map((s) => ({ value: s, labelKey: `rename:params.sortBy.${s}` })),
      },
      { kind: 'switch', key: 'sortDesc', labelKey: 'rename:params.sortDesc.label', visibleIf: (p) => p.sortBy !== 'none' },
    ],
  },
  {
    kind: 'group',
    id: 'rename.findReplace',
    labelKey: 'rename:groups.findReplace',
    descKey: 'rename:params.findReplace.desc',
    fields: [{ kind: 'custom', key: 'findReplace', render: (props) => <RulesControl {...props} /> }],
  },
  {
    kind: 'group',
    id: 'rename.smart',
    labelKey: 'rename:groups.smart',
    fields: [{ kind: 'custom', key: 'smart', render: (props) => <SmartControl {...props} /> }],
  },
  {
    kind: 'group',
    id: 'rename.extension',
    labelKey: 'rename:groups.extension',
    fields: [
      { kind: 'switch', key: 'keepExtension', labelKey: 'rename:params.keepExtension.label', descKey: 'rename:params.keepExtension.desc' },
      {
        kind: 'segmented',
        key: 'extensionCase',
        labelKey: 'rename:params.extensionCase.label',
        options: EXTENSION_CASES.map((c) => ({ value: c, labelKey: `rename:params.extensionCase.${c}` })),
        visibleIf: (p) => p.keepExtension !== false,
      },
      { kind: 'switch', key: 'sanitize', labelKey: 'rename:params.sanitize.label', descKey: 'rename:params.sanitize.desc' },
      {
        kind: 'text',
        key: 'invalidCharReplacement',
        labelKey: 'rename:params.invalidCharReplacement.label',
        monospace: true,
        visibleIf: (p) => p.sanitize === true,
      },
    ],
  },
  {
    kind: 'group',
    id: 'rename.destination',
    labelKey: 'rename:groups.destination',
    fields: [
      {
        kind: 'segmented',
        key: 'mode',
        labelKey: 'rename:params.mode.label',
        options: [
          { value: 'inPlace', labelKey: 'rename:params.mode.inPlace' },
          { value: 'copyTo', labelKey: 'rename:params.mode.copyTo' },
        ],
      },
      {
        kind: 'custom',
        key: 'copyDir',
        labelKey: 'rename:params.copyDir.label',
        descKey: 'rename:params.copyDir.desc',
        visibleIf: (p) => p.mode === 'copyTo',
        render: (props) => <FolderControl {...props} />,
      },
    ],
  },
])
