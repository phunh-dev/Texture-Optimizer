import { isTauri } from '@tauri-apps/api/core'

/** True when running inside the Tauri webview (false in the browser / tests). */
export function inTauri(): boolean {
  try {
    return isTauri()
  } catch {
    return false
  }
}
