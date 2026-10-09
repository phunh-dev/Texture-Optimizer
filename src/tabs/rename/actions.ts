// Execute / revert flows shared by the Rename button, the "Revert" toast action
// and "Revert last rename". They live outside React so a toast action still
// works after the tab went to sleep.
//
// Session consistency: a rename changes the disk, which undo/redo cannot
// revert. After an in-place rename (or a revert) the session's files are
// swapped to their new paths with `replaceFiles`, which creates NO history
// step and rewrites the existing undo/redo states to the new paths, so undo
// never resurrects paths that no longer exist. The disk itself is reverted only
// through the rename log ("Revert" toast / "Revert last rename").
import { toast } from 'sonner'

import i18n from '@/i18n'
import { translateError } from '@/lib/errors'
import { getSession } from '@/stores/session'
import { useTabs } from '@/stores/tabs'

import { renameExecute, renameRevert, renameRevertLast, type FileUpdate, type RevertOutcome } from './api'
import { usePlanRefresh } from './plan'
import type { ExecuteMode, RenameParams } from './schema'

/** Moves the renamed files in every open tab that lists them (the disk changed for all of them). */
function applyUpdates(tabId: string, updates: FileUpdate[]): void {
  if (updates.length > 0) {
    const ids = new Set([tabId, ...useTabs.getState().tabs.map((tab) => tab.id)])
    for (const id of ids) getSession(id)?.getState().replaceFiles(updates)
  }
  usePlanRefresh.getState().bump()
}

/** Runs the rename; resolves true on success. Shows result / error toasts. */
export async function executeRename(tabId: string, paths: string[], params: RenameParams, mode: ExecuteMode): Promise<boolean> {
  const t = i18n.t
  try {
    const out = await renameExecute(paths, params, mode)
    applyUpdates(tabId, out.updates)
    const count = out.log.entries.length
    toast.success(mode.kind === 'copyTo' ? t('rename:toast.copied', { count }) : t('rename:toast.renamed', { count }), {
      id: `rename-${tabId}`,
      action: { label: t('rename:actions.revert'), onClick: () => void revertRename(tabId, out.log.id) },
    })
    return true
  } catch (err) {
    usePlanRefresh.getState().bump()
    toast.error(translateError(err))
    return false
  }
}

async function runRevert(tabId: string, op: () => Promise<RevertOutcome>): Promise<boolean> {
  const t = i18n.t
  try {
    const out = await op()
    applyUpdates(tabId, out.updates)
    const count = out.log.entries.length
    toast.success(out.log.mode.kind === 'copyTo' ? t('rename:toast.copiesRemoved', { count }) : t('rename:toast.reverted', { count }), {
      id: `rename-${tabId}`,
    })
    return true
  } catch (err) {
    toast.error(translateError(err))
    return false
  }
}

/** Reverts one specific rename (the toast's "Revert" action). */
export const revertRename = (tabId: string, logId: string) => runRevert(tabId, () => renameRevert(logId))

/** Reverts the most recent rename that has not been reverted yet. */
export const revertLastRename = (tabId: string) => runRevert(tabId, renameRevertLast)
