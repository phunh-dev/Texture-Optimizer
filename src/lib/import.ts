// Import pipeline shared by the file picker, folder picker, "+" tile and
// drag & drop: paths -> scanPaths -> session.addFiles -> summary toast.
import { open } from '@tauri-apps/plugin-dialog'
import { toast } from 'sonner'

import i18n from '@/i18n'
import { scanPaths } from '@/lib/ipc'
import { requireSession } from '@/stores/session'
import { useSettings } from '@/stores/settings'

import { inTauri } from './env'
import { translateError } from './errors'

export const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'tga', 'bmp', 'webp', 'gif', 'tif', 'tiff'] as const

export interface ImportOptions {
  recursive?: boolean
}

export interface ImportSummary {
  added: number
  duplicates: number
  skipped: number
}

/** Scans paths (files and/or folders) and adds the images to the tab's session. */
export async function importPaths(tabId: string, paths: string[], options: ImportOptions = {}): Promise<ImportSummary | null> {
  if (paths.length === 0) return null
  const recursive = options.recursive ?? useSettings.getState().recursiveImport
  try {
    const result = await scanPaths(paths, { recursive })
    const added = requireSession(tabId).getState().addFiles(result.files)
    const summary = { added, duplicates: result.files.length - added, skipped: result.skipped.length }
    notify(summary, result.skipped.map((s) => s.error))
    return summary
  } catch (err) {
    toast.error(translateError(err))
    return null
  }
}

function notify(summary: ImportSummary, errors: unknown[]): void {
  const t = i18n.t
  if (summary.skipped > 0) {
    const first = errors[0] ? translateError(errors[0]) : undefined
    toast.warning(t('common:import.addedWithSkipped', { count: summary.added, skipped: summary.skipped }), {
      description: first,
    })
  } else if (summary.added > 0) {
    toast.success(t('common:import.added', { count: summary.added }))
  } else if (summary.duplicates > 0) {
    toast.info(t('common:import.allDuplicates', { count: summary.duplicates }))
  } else {
    toast.info(t('common:import.nothingFound'))
  }
}

function desktopOnly(): boolean {
  if (inTauri()) return false
  toast.info(i18n.t('common:import.desktopOnly'))
  return true
}

/** Opens the native multi-file picker and imports the selection. */
export async function importFromFilePicker(tabId: string): Promise<void> {
  if (desktopOnly()) return
  try {
    const selected = await open({
      multiple: true,
      directory: false,
      filters: [{ name: i18n.t('common:import.imagesFilter'), extensions: [...IMAGE_EXTENSIONS] }],
    })
    if (!selected) return
    await importPaths(tabId, Array.isArray(selected) ? selected : [selected])
  } catch (err) {
    toast.error(translateError(err))
  }
}

/** Opens the native folder picker and imports every image found (recursive per settings). */
export async function importFromFolderPicker(tabId: string): Promise<void> {
  if (desktopOnly()) return
  const recursive = useSettings.getState().recursiveImport
  try {
    const selected = await open({ directory: true, multiple: false, recursive })
    if (!selected) return
    await importPaths(tabId, Array.isArray(selected) ? selected : [selected], { recursive })
  } catch (err) {
    toast.error(translateError(err))
  }
}

/** Opens the native folder picker for an output directory. */
export async function pickOutputFolder(): Promise<string | null> {
  if (desktopOnly()) return null
  try {
    const selected = await open({ directory: true, multiple: false })
    return typeof selected === 'string' ? selected : null
  } catch (err) {
    toast.error(translateError(err))
    return null
  }
}
