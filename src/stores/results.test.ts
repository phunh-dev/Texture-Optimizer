import { beforeEach, describe, expect, it, vi } from 'vitest'

import { discardResults, listResults } from '@/lib/ipc'
import type { JobFinishedEvent, StagedResult } from '@/lib/ipc/types'
import { makeFile } from '@/lib/testing/dom'

import { useJobs } from './jobs'
import { beginStagedRun, dirName, joinPath, resultSignature, setResultsView, useResults } from './results'
import { getSession, resetSessions } from './session'
import { useTabs } from './tabs'

vi.mock('sonner', () => ({ toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn(), info: vi.fn(), warning: vi.fn() }) }))
vi.mock('@/lib/env', () => ({ inTauri: () => true }))
vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  releaseSession: vi.fn(() => Promise.resolve()),
  cancelJob: vi.fn(() => Promise.resolve()),
  listResults: vi.fn(),
  discardResults: vi.fn(() => Promise.resolve()),
}))

const finished = (tabId: string, jobId = 'job-1'): JobFinishedEvent => ({
  jobId,
  tabId,
  cancelled: false,
  results: [{ input: 'D:/a/x.png', output: `C:/stage/${tabId}/${jobId}/x.png`, error: null, meta: null }],
})
const listed = (tabId: string, jobId = 'job-1'): StagedResult[] => [
  { file: makeFile({ name: 'x.png', path: `C:/stage/${tabId}/${jobId}/x.png` }), sidecar: null },
]

async function stagedRun(tabId: string, jobId = 'job-1') {
  beginStagedRun(tabId, resultSignature(getSession(tabId)!.getState()))
  await useJobs.getState().start(tabId, 1, () => Promise.resolve(jobId))
  useJobs.getState().handleFinished(finished(tabId, jobId))
}

describe('results store', () => {
  beforeEach(() => {
    resetSessions()
    useTabs.setState({ tabs: [], activeTabId: null })
    useJobs.setState({ byTab: {} })
    useResults.setState({ byTab: {}, saving: {} })
    vi.mocked(listResults).mockReset()
    vi.mocked(discardResults).mockClear()
  })

  it('collects the staged results of a finished run', async () => {
    const tab = useTabs.getState().openTab('resize')
    vi.mocked(listResults).mockResolvedValue(listed(tab))
    await stagedRun(tab)
    await vi.waitFor(() => expect(useResults.getState().byTab[tab]).toBeDefined())
    const r = useResults.getState().byTab[tab]
    expect(r).toMatchObject({ jobId: 'job-1', failed: 0, view: 'result' })
    expect(r.items.map((i) => [i.source, i.file.path])).toEqual([['D:/a/x.png', `C:/stage/${tab}/job-1/x.png`]])
    setResultsView(tab, 'original')
    expect(useResults.getState().byTab[tab].view).toBe('original')
  })

  it('closing a tab deletes its staged results (backend + store)', async () => {
    const tab = useTabs.getState().openTab('resize')
    const other = useTabs.getState().openTab('trim')
    vi.mocked(listResults).mockResolvedValue(listed(tab))
    await stagedRun(tab)
    await vi.waitFor(() => expect(useResults.getState().byTab[tab]).toBeDefined())

    useTabs.getState().closeTab(tab)
    expect(discardResults).toHaveBeenCalledWith(tab)
    expect(discardResults).not.toHaveBeenCalledWith(other)
    expect(useResults.getState().byTab[tab]).toBeUndefined()
  })

  it('ignores a late listing when the results were dropped meanwhile', async () => {
    const tab = useTabs.getState().openTab('resize')
    let resolve!: (v: StagedResult[]) => void
    vi.mocked(listResults).mockReturnValue(new Promise((r) => (resolve = r)))
    await stagedRun(tab)
    useTabs.getState().closeTab(tab)
    resolve(listed(tab))
    await Promise.resolve()
    await Promise.resolve()
    expect(useResults.getState().byTab[tab]).toBeUndefined()
  })

  it('builds default save paths with the source separator style', () => {
    expect(dirName('D:/art/ui/a.png')).toBe('D:/art/ui')
    expect(dirName('D:\\art\\a.png')).toBe('D:\\art')
    expect(dirName('/a.png')).toBe('/')
    expect(dirName('a.png')).toBe('')
    expect(joinPath('D:/art', 'a.png')).toBe('D:/art/a.png')
    expect(joinPath('D:\\art', 'a.png')).toBe('D:\\art\\a.png')
    expect(joinPath('/', 'a.png')).toBe('/a.png')
    expect(joinPath('', 'a.png')).toBe('a.png')
  })
})
