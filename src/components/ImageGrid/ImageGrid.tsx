import { useVirtualizer } from '@tanstack/react-virtual'
import { PlusIcon } from 'lucide-react'
import { useCallback, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { useElementSize } from '@/hooks/useElementSize'
import { removeFilesWithUndo } from '@/lib/history'
import { importFromFilePicker, importFromFolderPicker } from '@/lib/import'
import type { ImportedFile } from '@/lib/ipc/types'
import { cn, hasModifier } from '@/lib/utils'
import { requireSession, useSession } from '@/stores/session'
import { useUi } from '@/stores/ui'

import { EmptyDropZone } from './EmptyDropZone'
import { ImageCell } from './ImageCell'
import { columnCount, GRID_PADDING, GRID_SIZES } from './sizes'

export interface ImageGridProps {
  tabId: string
  /** Override the "+" tile / "Add files" action (defaults to the native file picker). */
  onAddFiles?: () => void
  onAddFolder?: () => void
  className?: string
}

/**
 * Virtualised, row-based thumbnail grid of the tab's imported files.
 * Click / Ctrl+click / Shift+click select, Delete removes (undoable), the
 * last cell is a "+" tile that opens the file picker.
 */
export function ImageGrid({ tabId, onAddFiles, onAddFolder, className }: ImageGridProps) {
  const { t } = useTranslation('common')
  const files = useSession(tabId, (s) => s.files)
  const selectedIds = useSession(tabId, (s) => s.selectedIds)
  const viewSize = useSession(tabId, (s) => s.viewSize)
  const dragOver = useUi((s) => s.dragOver)

  const addFiles = useCallback(() => (onAddFiles ? onAddFiles() : void importFromFilePicker(tabId)), [onAddFiles, tabId])
  const addFolder = useCallback(() => (onAddFolder ? onAddFolder() : void importFromFolderPicker(tabId)), [onAddFolder, tabId])

  if (files.length === 0) {
    return (
      <div className={cn('relative size-full', className)}>
        <EmptyDropZone dragOver={dragOver} onAddFiles={addFiles} onAddFolder={addFolder} />
      </div>
    )
  }

  return (
    <VirtualGrid
      tabId={tabId}
      files={files}
      selectedIds={selectedIds}
      viewSize={viewSize}
      dragOver={dragOver}
      onAddFiles={addFiles}
      className={className}
      label={t('grid.label')}
      addLabel={t('grid.addTile')}
      dropLabel={t('import.dropActive')}
    />
  )
}

interface VirtualGridProps {
  tabId: string
  files: ImportedFile[]
  selectedIds: string[]
  viewSize: 'small' | 'medium' | 'large'
  dragOver: boolean
  onAddFiles: () => void
  className?: string
  label: string
  addLabel: string
  dropLabel: string
}

function VirtualGrid({ tabId, files, selectedIds, viewSize, dragOver, onAddFiles, className, label, addLabel, dropLabel }: VirtualGridProps) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const { width } = useElementSize(scrollRef)
  const metrics = GRID_SIZES[viewSize]
  const columns = columnCount(width, viewSize)
  const itemCount = files.length + 1 // + the "+" tile
  const rowCount = Math.ceil(itemCount / columns)
  const rowHeight = metrics.cell + metrics.caption
  const [focusIndex, setFocusIndex] = useState<number | null>(null)
  const selected = new Set(selectedIds)

  // eslint-disable-next-line react-hooks/incompatible-library -- virtualizer state is read fresh every render
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight + metrics.gap,
    overscan: 4,
    paddingStart: GRID_PADDING,
    paddingEnd: GRID_PADDING,
  })

  // Row height depends on the size mode.
  useEffect(() => {
    virtualizer.measure()
  }, [rowHeight, metrics.gap, virtualizer])

  // Restore the scroll position after waking up; remember it while scrolling.
  useLayoutEffect(() => {
    const el = scrollRef.current
    const saved = requireSession(tabId).getState().scrollTop
    if (el && saved > 0) el.scrollTop = saved
  }, [tabId])

  const onSelect = useCallback(
    (file: ImportedFile, e: MouseEvent) => {
      const session = requireSession(tabId).getState()
      const mode = e.shiftKey ? 'range' : hasModifier(e) ? 'toggle' : 'replace'
      session.select(file.id, mode)
      setFocusIndex(session.files.findIndex((f) => f.id === file.id))
      scrollRef.current?.focus({ preventScroll: true })
    },
    [tabId],
  )

  const onRemove = useCallback((file: ImportedFile) => removeFilesWithUndo(tabId, [file.id]), [tabId])

  const moveFocus = (next: number, e: KeyboardEvent) => {
    const index = Math.max(0, Math.min(files.length - 1, next))
    setFocusIndex(index)
    virtualizer.scrollToIndex(Math.floor(index / columns))
    const session = requireSession(tabId).getState()
    session.select(files[index].id, e.shiftKey ? 'range' : 'replace')
  }

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const session = requireSession(tabId).getState()
    const current = focusIndex ?? -1
    switch (e.key) {
      case 'Delete':
      case 'Backspace': {
        if (session.selectedIds.length === 0) return
        e.preventDefault()
        const first = files.findIndex((f) => session.selectedIds.includes(f.id))
        removeFilesWithUndo(tabId, session.selectedIds)
        const remaining = requireSession(tabId).getState().files.length
        setFocusIndex(first >= 0 && remaining > 0 ? Math.min(first, remaining - 1) : null)
        return
      }
      case 'a':
      case 'A':
        if (hasModifier(e)) {
          e.preventDefault()
          session.selectAll()
        }
        return
      case 'Escape':
        session.clearSelection()
        return
      case 'ArrowRight':
        e.preventDefault()
        moveFocus(current + 1, e)
        return
      case 'ArrowLeft':
        e.preventDefault()
        moveFocus(current - 1, e)
        return
      case 'ArrowDown':
        e.preventDefault()
        moveFocus(current < 0 ? 0 : current + columns, e)
        return
      case 'ArrowUp':
        e.preventDefault()
        moveFocus(current - columns, e)
        return
      case 'Home':
        e.preventDefault()
        moveFocus(0, e)
        return
      case 'End':
        e.preventDefault()
        moveFocus(files.length - 1, e)
        return
      case ' ':
        if (current >= 0 && current < files.length) {
          e.preventDefault()
          session.select(files[current].id, 'toggle')
        }
        return
    }
  }

  const domId = (index: number) => `grid-${tabId}-${index}`
  const activeDescendant = focusIndex != null && focusIndex < files.length ? domId(focusIndex) : undefined

  return (
    <div className={cn('relative size-full', className)}>
      <div
        ref={scrollRef}
        role="grid"
        aria-label={label}
        aria-multiselectable
        aria-rowcount={rowCount}
        aria-colcount={columns}
        aria-activedescendant={activeDescendant}
        tabIndex={0}
        data-testid="image-grid"
        data-columns={columns}
        onKeyDown={onKeyDown}
        onScroll={(e) => requireSession(tabId).getState().setScrollTop(e.currentTarget.scrollTop)}
        onClick={(e) => {
          if (e.target === e.currentTarget) requireSession(tabId).getState().clearSelection()
        }}
        className="size-full overflow-y-auto overflow-x-hidden outline-none [scrollbar-gutter:stable]"
        style={{ overscrollBehavior: 'contain' }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: 'relative', width: '100%' }}>
          {virtualizer.getVirtualItems().map((row) => {
            const start = row.index * columns
            const end = Math.min(itemCount, start + columns)
            return (
              <div
                key={row.key}
                role="row"
                aria-rowindex={row.index + 1}
                className="absolute left-0 top-0 grid w-full"
                style={{
                  transform: `translateY(${row.start}px)`,
                  height: rowHeight,
                  paddingLeft: GRID_PADDING,
                  paddingRight: GRID_PADDING,
                  gap: metrics.gap,
                  gridTemplateColumns: `repeat(${columns}, ${metrics.cell}px)`,
                }}
              >
                {Array.from({ length: end - start }, (_, i) => {
                  const index = start + i
                  if (index === files.length) {
                    return <AddTile key="__add" size={metrics.cell} height={rowHeight} label={addLabel} onClick={onAddFiles} compact={viewSize === 'small'} />
                  }
                  const file = files[index]
                  return (
                    <ImageCell
                      key={file.id}
                      file={file}
                      domId={domId(index)}
                      size={viewSize}
                      selected={selected.has(file.id)}
                      focused={focusIndex === index}
                      onSelect={onSelect}
                      onRemove={onRemove}
                    />
                  )
                })}
              </div>
            )
          })}
        </div>
      </div>

      {dragOver && (
        <div className="pointer-events-none absolute inset-2 z-10 flex items-center justify-center rounded-xl border-2 border-dashed border-primary bg-primary/10 backdrop-blur-[1px]">
          <span className="rounded-full bg-primary px-4 py-2 text-sm font-medium text-primary-foreground shadow-lg">{dropLabel}</span>
        </div>
      )}
    </div>
  )
}

function AddTile({ size, height, label, onClick, compact }: { size: number; height: number; label: string; onClick: () => void; compact: boolean }) {
  return (
    <div role="gridcell" style={{ width: size, height }}>
      <button
        type="button"
        aria-label={label}
        data-testid="add-tile"
        onClick={onClick}
        className="group/add flex size-full flex-col items-center justify-center gap-2 rounded-lg border-2 border-dashed border-border text-muted-foreground transition-colors outline-none hover:border-primary hover:bg-primary/5 hover:text-primary focus-visible:border-primary focus-visible:ring-2 focus-visible:ring-ring/40"
      >
        <span className="flex size-10 items-center justify-center rounded-full bg-muted transition-colors group-hover/add:bg-primary/10">
          <PlusIcon className="size-5" />
        </span>
        {compact ? null : <span className="text-xs font-medium">{label}</span>}
      </button>
    </div>
  )
}
