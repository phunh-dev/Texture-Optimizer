import { beforeEach, describe, expect, it, vi } from 'vitest'

import { cancelJob } from '@/lib/ipc'
import type { JobFinishedEvent } from '@/lib/ipc/types'

import { setJobFinishHandler, useJobs } from './jobs'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  cancelJob: vi.fn(() => Promise.resolve()),
}))

const finished = (jobId: string, tabId: string, n = 2): JobFinishedEvent => ({
  jobId,
  tabId,
  cancelled: false,
  results: Array.from({ length: n }, (_, i) => ({ input: `C:/${i}.png`, output: `C:/${i}_opt.png`, error: null, meta: null })),
})

describe('jobs store', () => {
  beforeEach(() => {
    useJobs.setState({ byTab: {} })
    vi.mocked(cancelJob).mockClear()
  })

  it('tracks progress per tab and finishes', async () => {
    await useJobs.getState().start('tab-a', 2, () => Promise.resolve('job-1'))
    expect(useJobs.getState().byTab['tab-a']).toMatchObject({ jobId: 'job-1', status: 'running', total: 2 })
    useJobs.getState().handleProgress({ jobId: 'job-1', tabId: 'tab-a', done: 1, total: 2, currentPath: 'C:/0.png' })
    expect(useJobs.getState().byTab['tab-a']).toMatchObject({ done: 1, currentPath: 'C:/0.png' })
    useJobs.getState().handleFinished(finished('job-1', 'tab-a'))
    expect(useJobs.getState().byTab['tab-a']).toMatchObject({ status: 'done', done: 2 })
  })

  it('accepts progress and finished events that arrive before runOp resolves', async () => {
    let resolve!: (id: string) => void
    const pending = useJobs.getState().start('tab-a', 2, () => new Promise<string>((r) => (resolve = r)))
    useJobs.getState().handleProgress({ jobId: 'job-9', tabId: 'tab-a', done: 1, total: 2, currentPath: null })
    useJobs.getState().handleFinished(finished('job-9', 'tab-a'))
    expect(useJobs.getState().byTab['tab-a'].status).toBe('done')
    resolve('job-9')
    await pending
    expect(useJobs.getState().byTab['tab-a']).toMatchObject({ jobId: 'job-9', status: 'done' })
  })

  it('buffers a finished event for an unknown job id until the id is known', async () => {
    // Previous job of the same tab finished; a new job's finished event comes early
    useJobs.setState({
      byTab: { 'tab-a': { jobId: 'old', status: 'done', done: 1, total: 1, currentPath: null, results: [], startedAt: 0 } },
    })
    useJobs.getState().handleFinished(finished('job-2', 'tab-a', 3))
    expect(useJobs.getState().byTab['tab-a'].jobId).toBe('old')
    await useJobs.getState().start('tab-a', 3, () => Promise.resolve('job-2'))
    expect(useJobs.getState().byTab['tab-a']).toMatchObject({ jobId: 'job-2', status: 'done', done: 3 })
  })

  it('ignores events of other jobs and cancels through the backend', async () => {
    await useJobs.getState().start('tab-a', 2, () => Promise.resolve('job-1'))
    useJobs.getState().handleProgress({ jobId: 'other', tabId: 'tab-a', done: 2, total: 2, currentPath: null })
    expect(useJobs.getState().byTab['tab-a'].done).toBe(0)
    await useJobs.getState().cancel('tab-a')
    expect(cancelJob).toHaveBeenCalledWith('job-1')
  })

  it('marks the job failed when runOp rejects', async () => {
    const id = await useJobs.getState().start('tab-a', 1, () => Promise.reject({ code: 'INVALID_PARAMS', params: {} }))
    expect(id).toBeNull()
    expect(useJobs.getState().byTab['tab-a'].status).toBe('failed')
  })

  it('a custom finish handler replaces the summary once', async () => {
    const handler = vi.fn()
    setJobFinishHandler('tab-a', handler)
    await useJobs.getState().start('tab-a', 2, () => Promise.resolve('job-1'))
    const e = finished('job-1', 'tab-a')
    useJobs.getState().handleFinished(e)
    expect(handler).toHaveBeenCalledWith(e)
    expect(useJobs.getState().byTab['tab-a'].status).toBe('done')
    await useJobs.getState().start('tab-a', 2, () => Promise.resolve('job-2'))
    useJobs.getState().handleFinished(finished('job-2', 'tab-a'))
    expect(handler).toHaveBeenCalledTimes(1)
  })
})
