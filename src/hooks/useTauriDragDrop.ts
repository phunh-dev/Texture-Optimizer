import { getCurrentWebview } from '@tauri-apps/api/webview'
import { useEffect } from 'react'

import { inTauri } from '@/lib/env'
import { importPaths } from '@/lib/import'
import { useTabs } from '@/stores/tabs'
import { useUi } from '@/stores/ui'

/**
 * Native file/folder drag & drop (real paths) via the Tauri webview. Highlights
 * the active tab's grid while dragging; a drop adds the files to that tab.
 */
export function useTauriDragDrop(): void {
  useEffect(() => {
    if (!inTauri()) return
    let disposed = false
    let unlisten: (() => void) | undefined
    getCurrentWebview()
      .onDragDropEvent((event) => {
        const { setDragOver } = useUi.getState()
        const activeTabId = useTabs.getState().activeTabId
        switch (event.payload.type) {
          case 'enter':
          case 'over':
            setDragOver(activeTabId != null)
            break
          case 'leave':
            setDragOver(false)
            break
          case 'drop':
            setDragOver(false)
            if (activeTabId) void importPaths(activeTabId, event.payload.paths)
            break
        }
      })
      .then((fn) => {
        if (disposed) fn()
        else unlisten = fn
      })
      .catch(() => undefined)
    return () => {
      disposed = true
      unlisten?.()
    }
  }, [])
}
