import 'i18next'

import type { resources } from './resources'

// Makes `t('ns:key')` type-checked against the English locale files.
declare module 'i18next' {
  interface CustomTypeOptions {
    defaultNS: 'common'
    resources: (typeof resources)['en']
  }
}
