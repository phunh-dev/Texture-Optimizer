// Test-only helpers for the image-op tabs (imported by *.test.tsx files only).
import { act, render } from '@testing-library/react'
import type { ComponentType } from 'react'
import { vi } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import type { ImportedFile, PreviewResult } from '@/lib/ipc/types'
import { makeFile } from '@/lib/testing/dom'
import { useJobs } from '@/stores/jobs'
import { getSession, resetSessions, type SessionStore } from '@/stores/session'
import { useTabs } from '@/stores/tabs'
import type { ToolId, ToolTabProps } from '@/tabs/registry'

import { PREVIEW_DEBOUNCE_MS } from './usePreview'

export function resetTabStores(): void {
  resetSessions()
  useTabs.setState({ tabs: [], activeTabId: null })
  useJobs.setState({ byTab: {} })
}

/** Opens a real tab of `toolId` (session seeded with the registry defaults). */
export function openTool(toolId: ToolId): { tabId: string; session: SessionStore } {
  const tabId = useTabs.getState().openTab(toolId)
  return { tabId, session: getSession(tabId)! }
}

export function renderTab(Tab: ComponentType<ToolTabProps>, tabId: string) {
  return render(
    <TooltipProvider>
      <Tab tabId={tabId} />
    </TooltipProvider>,
  )
}

/** The FieldShell wrapper of a param (null when hidden). */
export function fieldOf(tabId: string, key: string): HTMLElement | null {
  return document.querySelector(`[data-field-id="param-${tabId}-${key}"]`)
}

/** Visible param keys, in render order. */
export function visibleKeys(tabId: string): string[] {
  const prefix = `param-${tabId}-`
  return [...document.querySelectorAll('[data-field-id]')].map((el) => el.getAttribute('data-field-id')!.slice(prefix.length))
}

export function sizedFile(width: number, height: number, overrides: Partial<ImportedFile> = {}): ImportedFile {
  const name = overrides.name ?? `tex_${width}x${height}.png`
  return makeFile({ id: `id-${name}`, name, path: `C:/textures/${name}`, width, height, ...overrides })
}

export function previewResult(width: number, height: number, meta: unknown = null): PreviewResult {
  return { png: new ArrayBuffer(8), width, height, meta }
}

/** Stub object URLs (missing in jsdom). Returns the createObjectURL mock. */
export function stubObjectUrls() {
  let n = 0
  const create = vi.fn(() => `blob:preview-${++n}`)
  Object.defineProperty(URL, 'createObjectURL', { configurable: true, writable: true, value: create })
  Object.defineProperty(URL, 'revokeObjectURL', { configurable: true, writable: true, value: vi.fn() })
  return create
}

/** Advance fake timers past the preview debounce and flush the resulting promises. */
export async function settlePreview(ms = PREVIEW_DEBOUNCE_MS): Promise<void> {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms)
  })
}
