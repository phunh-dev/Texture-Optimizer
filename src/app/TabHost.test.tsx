import { act, cleanup, render, screen, waitFor } from '@testing-library/react'
import { useEffect } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import { releaseSession } from '@/lib/ipc'
import { makeFiles } from '@/lib/testing/dom'
import { useJobs } from '@/stores/jobs'
import { getSession, resetSessions, useSession } from '@/stores/session'
import { useTabs } from '@/stores/tabs'

import { TabBar } from './TabBar'
import { TabHost } from './TabHost'

const mounted = new Set<string>()
const mountLog: string[] = []

function Probe({ tabId }: { tabId: string }) {
  const count = useSession(tabId, (s) => s.files.length)
  useEffect(() => {
    mounted.add(tabId)
    mountLog.push(`mount:${tabId}`)
    return () => {
      mounted.delete(tabId)
      mountLog.push(`unmount:${tabId}`)
    }
  }, [tabId])
  return <div data-testid={`probe-${tabId}`}>{count}</div>
}

vi.mock('@/lib/env', () => ({ inTauri: () => true }))
vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  releaseSession: vi.fn(() => Promise.resolve()),
  cancelJob: vi.fn(() => Promise.resolve()),
}))
vi.mock('@/tabs/registry', async () => {
  const { BoxIcon } = await import('lucide-react')
  const tool = (id: string) => ({
    id,
    titleKey: `tools.${id}.title`,
    descriptionKey: `tools.${id}.description`,
    icon: BoxIcon,
    load: () => Promise.resolve({ default: Probe }),
    defaultParams: () => ({ strength: 1 }),
  })
  const tools = [tool('resize'), tool('trim')]
  return { tools, getTool: (id: string) => tools.find((t) => t.id === id)! }
})

function resetStores() {
  useTabs.setState({ tabs: [], activeTabId: null })
  useJobs.setState({ byTab: {} })
  resetSessions()
  mounted.clear()
  mountLog.length = 0
  vi.mocked(releaseSession).mockClear()
}

describe('tab store', () => {
  beforeEach(resetStores)

  it('opens, activates and closes tabs; multiple tabs of the same tool get ordinals', () => {
    const { openTab, activateTab, closeTab } = useTabs.getState()
    const a = openTab('resize')
    const b = openTab('resize')
    const c = openTab('trim')
    expect(useTabs.getState().tabs.map((t) => t.ordinal)).toEqual([1, 2, 1])
    expect(useTabs.getState().activeTabId).toBe(c)
    expect(getSession(a)!.getState().params).toEqual({ strength: 1 })

    activateTab(a)
    expect(useTabs.getState().activeTabId).toBe(a)

    closeTab(a) // closing the active tab activates its right neighbour
    expect(useTabs.getState().activeTabId).toBe(b)
    expect(getSession(a)).toBeUndefined()

    closeTab(c)
    closeTab(b)
    expect(useTabs.getState().tabs).toEqual([])
    expect(useTabs.getState().activeTabId).toBeNull()
  })

  it('cycles and reorders tabs', () => {
    const { openTab, cycleTab, moveTab } = useTabs.getState()
    const a = openTab('resize')
    const b = openTab('trim')
    cycleTab(1)
    expect(useTabs.getState().activeTabId).toBe(a)
    cycleTab(-1)
    expect(useTabs.getState().activeTabId).toBe(b)
    moveTab(0, 1)
    expect(useTabs.getState().tabs.map((t) => t.id)).toEqual([b, a])
  })

  it('calls releaseSession for the tab that goes to sleep', () => {
    const { openTab, activateTab } = useTabs.getState()
    const a = openTab('resize')
    const b = openTab('trim')
    expect(releaseSession).toHaveBeenLastCalledWith(a)
    activateTab(a)
    expect(releaseSession).toHaveBeenLastCalledWith(b)
  })
})

describe('TabHost sleep / awake', () => {
  beforeEach(resetStores)
  afterEach(() => {
    cleanup()
    vi.useRealTimers()
  })

  it('mounts only the active tab and keeps sleeping tab state and history intact', async () => {
    const a = useTabs.getState().openTab('resize')
    render(<TabHost />)
    expect(await screen.findByTestId(`probe-${a}`)).toBeInTheDocument()

    act(() => {
      getSession(a)!.getState().addFiles(makeFiles(3))
      getSession(a)!.getState().removeFiles(['f1'])
    })
    expect(screen.getByTestId(`probe-${a}`)).toHaveTextContent('2')

    let b = ''
    act(() => {
      b = useTabs.getState().openTab('trim')
    })
    expect(await screen.findByTestId(`probe-${b}`)).toBeInTheDocument()

    // Previous tab is unmounted (asleep) ...
    expect(screen.queryByTestId(`probe-${a}`)).not.toBeInTheDocument()
    expect(mounted.has(a)).toBe(false)
    expect(mounted.has(b)).toBe(true)
    expect(mountLog).toContain(`unmount:${a}`)
    expect(releaseSession).toHaveBeenCalledWith(a)
    // ... but its state and undo history live on outside React.
    expect(getSession(a)!.getState().files.map((f) => f.id)).toEqual(['f0', 'f2'])
    expect(getSession(a)!.temporal.getState().pastStates).toHaveLength(2)

    act(() => useTabs.getState().activateTab(a))
    expect(await screen.findByTestId(`probe-${a}`)).toHaveTextContent('2')
    expect(screen.queryByTestId(`probe-${b}`)).not.toBeInTheDocument()
    expect(releaseSession).toHaveBeenCalledWith(b)

    // Undo after waking up still works.
    act(() => getSession(a)!.getState().undo())
    expect(screen.getByTestId(`probe-${a}`)).toHaveTextContent('3')
    expect(getSession(a)!.getState().files.map((f) => f.id)).toEqual(['f0', 'f1', 'f2'])
  })

  it('a sleeping busy tab keeps receiving progress shown as a badge in the tab bar', async () => {
    const a = useTabs.getState().openTab('resize')
    await act(async () => {
      await useJobs.getState().start(a, 4, () => Promise.resolve('job-1'))
    })
    const b = useTabs.getState().openTab('trim')
    render(
      <TooltipProvider>
        <TabBar />
        <TabHost />
      </TooltipProvider>,
    )
    expect(await screen.findByTestId(`probe-${b}`)).toBeInTheDocument()

    act(() => useJobs.getState().handleProgress({ jobId: 'job-1', tabId: a, done: 1, total: 4, currentPath: 'C:/x.png' }))
    const tab = screen.getByTestId(`tab-${a}`)
    await waitFor(() => expect(tab.querySelector('[data-testid="tab-progress"]')).toHaveTextContent('25%'))

    act(() => useJobs.getState().handleProgress({ jobId: 'job-1', tabId: a, done: 3, total: 4, currentPath: 'C:/y.png' }))
    expect(tab.querySelector('[data-testid="tab-progress"]')).toHaveTextContent('75%')

    act(() =>
      useJobs.getState().handleFinished({
        jobId: 'job-1',
        tabId: a,
        cancelled: false,
        results: [1, 2, 3, 4].map((i) => ({ input: `C:/${i}.png`, output: `C:/${i}_opt.png`, error: null, meta: null })),
      }),
    )
    expect(tab.querySelector('[data-testid="tab-progress"]')).toBeNull()
    expect(useJobs.getState().byTab[a].status).toBe('done')
  })

  it('closing a tab via its close button deletes its session', async () => {
    const a = useTabs.getState().openTab('resize')
    render(
      <TooltipProvider>
        <TabBar />
      </TooltipProvider>,
    )
    act(() => {
      screen.getByRole('button', { name: /Close/ }).click()
    })
    expect(useTabs.getState().tabs).toHaveLength(0)
    expect(getSession(a)).toBeUndefined()
  })

  it('middle-click closes a tab', () => {
    const a = useTabs.getState().openTab('resize')
    useTabs.getState().openTab('trim')
    render(
      <TooltipProvider>
        <TabBar />
      </TooltipProvider>,
    )
    const tab = screen.getByTestId(`tab-${a}`)
    act(() => {
      tab.dispatchEvent(new MouseEvent('auxclick', { bubbles: true, button: 1 }))
    })
    expect(useTabs.getState().tabs.map((t) => t.id)).not.toContain(a)
  })
})
