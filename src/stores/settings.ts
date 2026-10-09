import { useEffect, useState } from 'react'
import { create } from 'zustand'

import i18n, { detectLanguage, supportedLanguages, type Language } from '@/i18n'
import { loadValue, saveValue, SETTINGS_FILE } from '@/lib/persist'

export type Theme = 'light' | 'dark' | 'system'
export const themes: readonly Theme[] = ['light', 'dark', 'system']

interface SettingsState {
  language: Language
  theme: Theme
  /** Descend into sub-folders when importing a folder. */
  recursiveImport: boolean
  loaded: boolean
  init: () => Promise<void>
  setLanguage: (language: Language) => Promise<void>
  setTheme: (theme: Theme) => void
  setRecursiveImport: (value: boolean) => void
}

/** Applies the theme to <html data-theme>; "system" lets CSS follow the OS. */
export function applyTheme(theme: Theme): void {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  if (theme === 'system') root.removeAttribute('data-theme')
  else root.setAttribute('data-theme', theme)
}

function applyLanguage(language: Language): Promise<unknown> {
  if (typeof document !== 'undefined') document.documentElement.lang = language
  return i18n.changeLanguage(language)
}

const isLanguage = (v: unknown): v is Language => supportedLanguages.includes(v as Language)
const isTheme = (v: unknown): v is Theme => themes.includes(v as Theme)

export const useSettings = create<SettingsState>()((set) => ({
  language: detectLanguage(typeof navigator === 'undefined' ? undefined : navigator.language),
  theme: 'system',
  recursiveImport: true,
  loaded: false,

  init: async () => {
    const [language, theme, recursiveImport] = await Promise.all([
      loadValue<string>(SETTINGS_FILE, 'language'),
      loadValue<string>(SETTINGS_FILE, 'theme'),
      loadValue<boolean>(SETTINGS_FILE, 'recursiveImport'),
    ])
    const next = {
      language: isLanguage(language)
        ? language
        : detectLanguage(typeof navigator === 'undefined' ? undefined : navigator.language),
      theme: isTheme(theme) ? theme : 'system',
      recursiveImport: typeof recursiveImport === 'boolean' ? recursiveImport : true,
    }
    applyTheme(next.theme)
    await applyLanguage(next.language)
    set({ ...next, loaded: true })
  },

  setLanguage: async (language) => {
    set({ language })
    await applyLanguage(language)
    await saveValue(SETTINGS_FILE, 'language', language)
  },

  setTheme: (theme) => {
    set({ theme })
    applyTheme(theme)
    void saveValue(SETTINGS_FILE, 'theme', theme)
  },

  setRecursiveImport: (recursiveImport) => {
    set({ recursiveImport })
    void saveValue(SETTINGS_FILE, 'recursiveImport', recursiveImport)
  },
}))

function systemPrefersDark(): boolean {
  return typeof window !== 'undefined' && typeof window.matchMedia === 'function'
    ? window.matchMedia('(prefers-color-scheme: dark)').matches
    : false
}

/** The effective light/dark theme (resolves "system" and follows OS changes). */
export function useResolvedTheme(): 'light' | 'dark' {
  const theme = useSettings((s) => s.theme)
  const [systemDark, setSystemDark] = useState(systemPrefersDark)
  useEffect(() => {
    if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return
    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const onChange = () => setSystemDark(mq.matches)
    mq.addEventListener?.('change', onChange)
    return () => mq.removeEventListener?.('change', onChange)
  }, [])
  if (theme === 'system') return systemDark ? 'dark' : 'light'
  return theme
}
