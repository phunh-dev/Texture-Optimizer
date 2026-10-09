import { PlusIcon, SettingsIcon, XIcon } from 'lucide-react'
import { useRef, useState, type KeyboardEvent, type PointerEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { Spinner } from '@/components/ui/misc'
import { Tooltip } from '@/components/ui/tooltip'
import { cn } from '@/lib/utils'
import { isJobActive, useJobs } from '@/stores/jobs'
import { useTabs, type TabInfo } from '@/stores/tabs'
import { useUi } from '@/stores/ui'
import { getTool } from '@/tabs/registry'

import { useTabTitle } from './useTabTitle'

const DRAG_THRESHOLD = 6

/** Top tab strip: icon + title + busy badge + close; middle-click closes; drag to reorder. */
export function TabBar() {
  const { t } = useTranslation('tabs')
  const tabs = useTabs((s) => s.tabs)
  const activeTabId = useTabs((s) => s.activeTabId)
  const title = useTabTitle()
  const listRef = useRef<HTMLDivElement>(null)
  const drag = useRef<{ id: string; startX: number; moved: boolean } | null>(null)
  const [draggingId, setDraggingId] = useState<string | null>(null)

  const onPointerDown = (e: PointerEvent<HTMLDivElement>, tab: TabInfo) => {
    if (e.button !== 0) return
    drag.current = { id: tab.id, startX: e.clientX, moved: false }
  }

  const onPointerMove = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d) return
    if (!d.moved) {
      if (Math.abs(e.clientX - d.startX) < DRAG_THRESHOLD) return
      d.moved = true
      setDraggingId(d.id)
      e.currentTarget.setPointerCapture?.(e.pointerId)
    }
    // Find the tab under the pointer and move the dragged tab there.
    const items = Array.from(listRef.current?.querySelectorAll<HTMLElement>('[data-tab-id]') ?? [])
    const { tabs: current, moveTab } = useTabs.getState()
    const from = current.findIndex((x) => x.id === d.id)
    for (const el of items) {
      const rect = el.getBoundingClientRect()
      if (e.clientX >= rect.left && e.clientX <= rect.right) {
        const to = current.findIndex((x) => x.id === el.dataset.tabId)
        if (to >= 0 && to !== from) moveTab(from, to)
        break
      }
    }
  }

  const endDrag = () => {
    drag.current = null
    setDraggingId(null)
  }

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>, index: number) => {
    const { tabs: current, activateTab, closeTab } = useTabs.getState()
    let next = -1
    if (e.key === 'ArrowRight') next = (index + 1) % current.length
    if (e.key === 'ArrowLeft') next = (index - 1 + current.length) % current.length
    if (e.key === 'Home') next = 0
    if (e.key === 'End') next = current.length - 1
    if (next >= 0) {
      e.preventDefault()
      activateTab(current[next].id)
      listRef.current?.querySelector<HTMLElement>(`[data-tab-id="${current[next].id}"]`)?.focus()
    } else if (e.key === 'Delete') {
      e.preventDefault()
      closeTab(current[index].id)
    }
  }

  return (
    <header className="flex h-11 shrink-0 items-end gap-1 border-b border-border bg-muted/60 pl-2 pr-2" data-tauri-drag-region>
      <div
        ref={listRef}
        role="tablist"
        aria-label={t('bar.label')}
        className="flex min-w-0 items-end gap-0.5 overflow-x-auto [scrollbar-width:none]"
        onPointerMove={onPointerMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
      >
        {tabs.map((tab, index) => (
          <TabItem
            key={tab.id}
            tab={tab}
            title={title(tab)}
            active={tab.id === activeTabId}
            dragging={tab.id === draggingId}
            onPointerDown={(e) => onPointerDown(e, tab)}
            onKeyDown={(e) => onKeyDown(e, index)}
          />
        ))}
      </div>
      <Tooltip content={t('bar.newTab')} shortcut="Ctrl+T">
        <Button
          variant="ghost"
          size="icon-sm"
          className="mb-1 shrink-0"
          aria-label={t('bar.newTab')}
          onClick={() => useUi.getState().setToolPickerOpen(true)}
        >
          <PlusIcon />
        </Button>
      </Tooltip>
      <div className="flex-1 self-stretch" data-tauri-drag-region />
      <Tooltip content={t('bar.settings')}>
        <Button
          variant="ghost"
          size="icon-sm"
          className="mb-1 shrink-0"
          aria-label={t('bar.settings')}
          onClick={() => useUi.getState().setSettingsOpen(true)}
        >
          <SettingsIcon />
        </Button>
      </Tooltip>
    </header>
  )
}

interface TabItemProps {
  tab: TabInfo
  title: string
  active: boolean
  dragging: boolean
  onPointerDown: (e: PointerEvent<HTMLDivElement>) => void
  onKeyDown: (e: KeyboardEvent<HTMLDivElement>) => void
}

function TabItem({ tab, title, active, dragging, onPointerDown, onKeyDown }: TabItemProps) {
  const { t } = useTranslation('tabs')
  const job = useJobs((s) => s.byTab[tab.id])
  const busy = isJobActive(job)
  const percent = job && job.total > 0 ? Math.round((job.done / job.total) * 100) : 0
  const Icon = getTool(tab.toolId).icon

  return (
    <div
      role="tab"
      tabIndex={active ? 0 : -1}
      aria-selected={active}
      data-tab-id={tab.id}
      data-testid={`tab-${tab.id}`}
      title={title}
      onPointerDown={onPointerDown}
      onClick={() => useTabs.getState().activateTab(tab.id)}
      onAuxClick={(e) => {
        if (e.button === 1) {
          e.preventDefault()
          useTabs.getState().closeTab(tab.id)
        }
      }}
      onMouseDown={(e) => {
        // Prevent the middle-click autoscroll cursor.
        if (e.button === 1) e.preventDefault()
      }}
      onKeyDown={onKeyDown}
      className={cn(
        'group/tab relative flex h-9 w-48 min-w-28 shrink cursor-default select-none items-center gap-2 rounded-t-lg pl-3 pr-1.5 text-[13px] outline-none transition-colors',
        'focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring/60',
        active ? 'bg-background font-medium text-foreground shadow-[0_-1px_0_0_var(--border),1px_0_0_0_var(--border),-1px_0_0_0_var(--border)]' : 'text-muted-foreground hover:bg-background/60 hover:text-foreground',
        dragging && 'z-10 opacity-80 shadow-lg',
      )}
    >
      {busy ? (
        <Spinner className="size-3.5 shrink-0 text-primary" />
      ) : (
        <Icon className={cn('size-3.5 shrink-0', active ? 'text-primary' : '')} />
      )}
      <span className="min-w-0 flex-1 truncate">{title}</span>
      {busy ? (
        <span
          className="shrink-0 rounded-full bg-primary/15 px-1.5 text-[10px] font-semibold tabular-nums text-primary"
          aria-label={t('bar.busy', { percent })}
          data-testid="tab-progress"
        >
          {t('bar.percent', { percent })}
        </span>
      ) : null}
      <button
        type="button"
        tabIndex={-1}
        aria-label={t('bar.closeTab', { title })}
        onPointerDown={(e) => e.stopPropagation()}
        onClick={(e) => {
          e.stopPropagation()
          useTabs.getState().closeTab(tab.id)
        }}
        className={cn(
          'flex size-5 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-[opacity,background-color] hover:bg-foreground/10 hover:text-foreground',
          active ? 'opacity-100' : 'opacity-0 group-hover/tab:opacity-100',
        )}
      >
        <XIcon className="size-3.5" />
      </button>
      {active ? <span className="absolute inset-x-3 top-0 h-0.5 rounded-full bg-primary" /> : null}
    </div>
  )
}
