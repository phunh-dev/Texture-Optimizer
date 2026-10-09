import { useEffect } from 'react'

import { Toaster } from '@/components/ui/sonner'
import { TooltipProvider } from '@/components/ui/tooltip'
import { useGlobalShortcuts } from '@/hooks/useShortcuts'
import { useTauriDragDrop } from '@/hooks/useTauriDragDrop'
import { initJobEvents } from '@/stores/jobs'
import { useSettings } from '@/stores/settings'
import { useTabs } from '@/stores/tabs'

import { HomeScreen } from './HomeScreen'
import { SettingsDialog } from './SettingsDialog'
import { TabBar } from './TabBar'
import { TabHost } from './TabHost'
import { ToolPicker } from './ToolPicker'

/** Root layout: tab bar on top, the active tab (or the home screen) below. */
export function AppShell() {
  const hasTabs = useTabs((s) => s.tabs.length > 0)

  useGlobalShortcuts()
  useTauriDragDrop()

  useEffect(() => {
    void useSettings.getState().init()
    let dispose: (() => void) | undefined
    let cancelled = false
    void initJobEvents().then((fn) => {
      if (cancelled) fn()
      else dispose = fn
    })
    return () => {
      cancelled = true
      dispose?.()
    }
  }, [])

  return (
    <TooltipProvider delayDuration={400} skipDelayDuration={150}>
      <div className="flex h-screen flex-col overflow-hidden bg-background text-foreground">
        <TabBar />
        <main className="min-h-0 flex-1">{hasTabs ? <TabHost /> : <HomeScreen />}</main>
      </div>
      <ToolPicker />
      <SettingsDialog />
      <Toaster />
    </TooltipProvider>
  )
}
