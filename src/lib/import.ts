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

// Must match texopt_core::io::SUPPORTED_EXTENSIONS.
export const IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'tga', 'bmp', 'webp'] as const

export interface ImportOptions {
  recursive?: boolean
}

export interface ImportSummary {
  added: number
  duplicates: number
  skipped: number
}

/**
 * Custom import for tabs whose items are not images (e.g. 3D models). While
 * registered, drag & drop, the file/folder pickers and `importPaths` of that
 * tab go through it instead of the image scan.
 */
export interface TabImporter {
  importPaths: (tabId: string, paths: string[], options: Required<ImportOptions>) => Promise<ImportSummary | null>
  /** File picker filter: i18n key of its name and the accepted extensions. */
  filterNameKey: string
  extensions: readonly string[]
}

const tabImporters = new Map<string, TabImporter>()

/** Registers a custom importer for a tab; returns the unregister function. */
export function registerTabImporter(tabId: string, importer: TabImporter): () => void {
  tabImporters.set(tabId, importer)
  return () => {
    if (tabImporters.get(tabId) === importer) tabImporters.delete(tabId)
  }
}

export function getTabImporter(tabId: string): TabImporter | undefined {
  return tabImporters.get(tabId)
}

/** Scans paths (files and/or folders) and adds the images to the tab's session. */
export async function importPaths(tabId: string, paths: string[], options: ImportOptions = {}): Promise<ImportSummary | null> {
  if (paths.length === 0) return null
  const recursive = options.recursive ?? useSettings.getState().recursiveImport
  const custom = tabImporters.get(tabId)
  if (custom) return custom.importPaths(tabId, paths, { recursive })
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
    const custom = tabImporters.get(tabId)
    const t = i18n.t as unknown as (key: string) => string
    const filter = custom
      ? { name: t(custom.filterNameKey), extensions: [...custom.extensions] }
      : { name: i18n.t('common:import.imagesFilter'), extensions: [...IMAGE_EXTENSIONS] }
    const selected = await open({ multiple: true, directory: false, filters: [filter] })
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
