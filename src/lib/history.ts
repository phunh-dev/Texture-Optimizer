import { toast } from 'sonner'

import i18n from '@/i18n'
import { requireSession, type LastAction } from '@/stores/session'

type T = typeof i18n.t

/** Human label of a history step, e.g. "Remove 3 images". */
export function historyActionLabel(t: T, action: LastAction): string {
  const count = action.count ?? 1
  switch (action.key) {
    case 'addFiles':
      return t('common:history.addFiles', { count })
    case 'removeFiles':
      return t('common:history.removeFiles', { count })
    case 'clearFiles':
      return t('common:history.clearFiles', { count })
    case 'reorderFiles':
      return t('common:history.reorderFiles')
    case 'changeParams':
      return t('common:history.changeParams')
    case 'resetParams':
      return t('common:history.resetParams')
    case 'applyPreset':
      return t('common:history.applyPreset')
    case 'changeOutput':
      return t('common:history.changeOutput')
  }
}

/** "Undo: Remove 3 images" / "Nothing to undo". */
export function undoTooltip(t: T, action: LastAction | null): string {
  return action ? t('common:history.undoLabel', { action: historyActionLabel(t, action) }) : t('common:history.nothingToUndo')
}

export function redoTooltip(t: T, action: LastAction | null): string {
  return action ? t('common:history.redoLabel', { action: historyActionLabel(t, action) }) : t('common:history.nothingToRedo')
}

/** Removes files from a tab and offers an "Undo" toast action. */
export function removeFilesWithUndo(tabId: string, ids: string[]): number {
  const session = requireSession(tabId)
  const removed = session.getState().removeFiles(ids)
  if (removed > 0) announceRemoval(tabId, i18n.t('common:history.removedToast', { count: removed }))
  return removed
}

export function clearFilesWithUndo(tabId: string): number {
  const session = requireSession(tabId)
  const removed = session.getState().clearFiles()
  if (removed > 0) announceRemoval(tabId, i18n.t('common:history.clearedToast', { count: removed }))
  return removed
}

function announceRemoval(tabId: string, message: string): void {
  toast(message, {
    id: `remove-${tabId}`,
    action: {
      label: i18n.t('common:actions.undo'),
      onClick: () => requireSession(tabId).getState().undo(),
    },
  })
}
