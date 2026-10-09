// Typed wrappers around the renamer commands (src-tauri/src/commands_rename.rs).
import { invoke } from '@tauri-apps/api/core'

import type { ImportedFile } from '@/lib/ipc/types'

import type { ExecuteMode, RenameParams } from './schema'

export type Conflict = 'duplicateInBatch' | 'existsOnDisk' | 'invalidName'

export interface RenamePlanItem {
  from: string
  /** New path; in copy mode the destination inside the target folder. */
  to: string
  conflict: Conflict | null
}

export interface RenameLogEntry {
  from: string
  to: string
}

export interface StoredRenameLog {
  id: string
  entries: RenameLogEntry[]
  mode: ExecuteMode
  /** ms since the Unix epoch */
  timestamp: number
}

/** A session file that now lives at `file.path` instead of `from`. */
export interface FileUpdate {
  from: string
  file: ImportedFile
}

export interface ExecuteOutcome {
  log: StoredRenameLog
  /** In-place only: the renamed files (copy mode leaves the originals untouched). */
  updates: FileUpdate[]
}

export interface RevertOutcome {
  log: StoredRenameLog
  /** In-place: files moved back to their original paths. */
  updates: FileUpdate[]
  /** Copy mode: deleted copies. */
  removed: string[]
}

export function renamePlan(paths: string[], params: RenameParams, mode: ExecuteMode): Promise<RenamePlanItem[]> {
  return invoke('rename_plan', { paths, params, mode })
}

/** The backend re-plans from `paths` + `params` and refuses on conflicts (no client plan is trusted). */
export function renameExecute(paths: string[], params: RenameParams, mode: ExecuteMode): Promise<ExecuteOutcome> {
  return invoke('rename_execute', { paths, params, mode })
}

export function renameRevertLast(): Promise<RevertOutcome> {
  return invoke('rename_revert_last')
}

export function renameRevert(logId: string): Promise<RevertOutcome> {
  return invoke('rename_revert', { logId })
}

/** Persisted logs, newest first. */
export function renameLogs(): Promise<StoredRenameLog[]> {
  return invoke('rename_logs')
}
