// Small key/value persistence. Inside Tauri it uses @tauri-apps/plugin-store
// (one JSON file per `file`); in the browser and in tests it falls back to
// localStorage under `<file>:<key>`.
import { LazyStore } from '@tauri-apps/plugin-store'

import { inTauri } from './env'

const stores = new Map<string, LazyStore>()

function tauriStore(file: string): LazyStore {
  let store = stores.get(file)
  if (!store) {
    store = new LazyStore(file)
    stores.set(file, store)
  }
  return store
}

function storageKey(file: string, key: string): string {
  return `${file}:${key}`
}

export async function loadValue<T>(file: string, key: string): Promise<T | undefined> {
  if (inTauri()) {
    try {
      return await tauriStore(file).get<T>(key)
    } catch (err) {
      console.warn(`[persist] cannot read ${file}/${key}`, err)
      return undefined
    }
  }
  try {
    const raw = globalThis.localStorage?.getItem(storageKey(file, key))
    return raw == null ? undefined : (JSON.parse(raw) as T)
  } catch {
    return undefined
  }
}

export async function saveValue(file: string, key: string, value: unknown): Promise<void> {
  if (inTauri()) {
    try {
      const store = tauriStore(file)
      await store.set(key, value)
      await store.save()
    } catch (err) {
      console.warn(`[persist] cannot write ${file}/${key}`, err)
    }
    return
  }
  try {
    globalThis.localStorage?.setItem(storageKey(file, key), JSON.stringify(value))
  } catch {
    // Storage full or unavailable: settings simply are not persisted.
  }
}

export const SETTINGS_FILE = 'settings.json'
export const PRESETS_FILE = 'presets.json'
