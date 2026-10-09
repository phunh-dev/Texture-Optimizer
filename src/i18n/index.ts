import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'

import { namespaces, resources } from './resources'

export const supportedLanguages = ['en', 'vi'] as const
export type Language = (typeof supportedLanguages)[number]

export function detectLanguage(locale: string | undefined): Language {
  return locale?.toLowerCase().startsWith('vi') ? 'vi' : 'en'
}

void i18n.use(initReactI18next).init({
  resources,
  ns: namespaces,
  defaultNS: 'common',
  lng: detectLanguage(typeof navigator === 'undefined' ? undefined : navigator.language),
  fallbackLng: 'en',
  supportedLngs: supportedLanguages,
  interpolation: { escapeValue: false },
  returnNull: false,
  saveMissing: import.meta.env.DEV,
  missingKeyHandler: (lngs, ns, key) => {
    console.warn(`[i18n] missing key ${ns}:${key} for ${lngs.join(',')}`)
  },
})

export default i18n
