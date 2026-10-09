// Global job tracker. Lives outside React so a sleeping (unmounted) tab keeps
// receiving progress and the tab bar badge stays live.
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { toast } from 'sonner'
import { create } from 'zustand'

import i18n from '@/i18n'
import { inTauri } from '@/lib/env'
import { translateError } from '@/lib/errors'
import { cancelJob } from '@/lib/ipc'
import {
  JOB_FINISHED_EVENT,
  JOB_PROGRESS_EVENT,
  type JobFileResult,
  type JobFinishedEvent,
  type JobProgressEvent,
} from '@/lib/ipc/types'

export type JobStatus = 'starting' | 'running' | 'done' | 'cancelled' | 'failed'

export interface JobState {
  jobId: string | null
  status: JobStatus
  done: number
  total: number
  currentPath: string | null
  results: JobFileResult[]
  startedAt: number
}

interface JobsStore {
  byTab: Record<string, JobState>
  /** Starts a job for a tab. `start` performs the IPC call and resolves with the job id. */
  start: (tabId: string, total: number, start: () => Promise<string>) => Promise<string | null>
  cancel: (tabId: string) => Promise<void>
  handleProgress: (e: JobProgressEvent) => void
  /** Applies a finished event; events for not-yet-known job ids are buffered. */
  handleFinished: (e: JobFinishedEvent) => void
  forgetTab: (tabId: string, options?: { cancel?: boolean }) => void
}

export const isJobActive = (job: JobState | undefined): boolean =>
  !!job && (job.status === 'starting' || job.status === 'running')

function summarize(e: JobFinishedEvent): void {
  const t = i18n.t
  if (e.cancelled) {
    toast.info(t('common:jobs.cancelled'))
    return
  }
  const failed = e.results.filter((r) => r.error)
  const ok = e.results.length - failed.length
  if (failed.length === 0) {
    toast.success(t('common:jobs.finished', { count: ok }))
    return
  }
  const details = failed
    .slice(0, 3)
    .map((r) => `${r.input.split(/[\\/]/).pop()}: ${translateError(r.error)}`)
    .join('\n')
  const more = failed.length > 3 ? `\n${t('common:jobs.moreErrors', { count: failed.length - 3 })}` : ''
  toast.error(t('common:jobs.finishedWithErrors', { ok, failed: failed.length }), { description: details + more })
}

/** A job state accepts events of its own job, or of any job while its id is still unknown. */
function matches(job: JobState | undefined, jobId: string): job is JobState {
  return !!job && (job.jobId === null || job.jobId === jobId)
}

// Finished events received before the matching runOp call resolved.
const earlyFinished = new Map<string, JobFinishedEvent>()
function rememberEarly(e: JobFinishedEvent): void {
  earlyFinished.set(e.jobId, e)
  if (earlyFinished.size > 32) earlyFinished.delete(earlyFinished.keys().next().value as string)
}

export const useJobs = create<JobsStore>()((set, get) => ({
  byTab: {},

  start: async (tabId, total, start) => {
    if (isJobActive(get().byTab[tabId])) return null
    set((s) => ({
      byTab: {
        ...s.byTab,
        [tabId]: { jobId: null, status: 'starting', done: 0, total, currentPath: null, results: [], startedAt: Date.now() },
      },
    }))
    try {
      const jobId = await start()
      const job = get().byTab[tabId]
      if (job && !job.jobId) {
        // Events may have arrived before the command resolved (tiny jobs).
        set((s) => ({ byTab: { ...s.byTab, [tabId]: { ...job, jobId, status: job.status === 'starting' ? 'running' : job.status } } }))
      }
      const early = earlyFinished.get(jobId)
      if (early) {
        earlyFinished.delete(jobId)
        get().handleFinished(early)
      }
      return jobId
    } catch (err) {
      const job = get().byTab[tabId]
      if (job) set((s) => ({ byTab: { ...s.byTab, [tabId]: { ...job, status: 'failed' } } }))
      toast.error(translateError(err))
      return null
    }
  },

  cancel: async (tabId) => {
    const job = get().byTab[tabId]
    if (!job?.jobId || !isJobActive(job)) return
    try {
      await cancelJob(job.jobId)
    } catch (err) {
      toast.error(translateError(err))
    }
  },

  handleProgress: (e) => {
    const job = get().byTab[e.tabId]
    if (!matches(job, e.jobId) || !isJobActive(job)) return
    set((s) => ({
      byTab: {
        ...s.byTab,
        [e.tabId]: { ...job, jobId: e.jobId, status: 'running', done: e.done, total: e.total, currentPath: e.currentPath },
      },
    }))
  },

  handleFinished: (e) => {
    const job = get().byTab[e.tabId]
    if (!matches(job, e.jobId) || !isJobActive(job)) {
      // Unknown job id yet: keep it until runOp resolves with that id.
      if (!job || job.jobId !== e.jobId) rememberEarly(e)
      return
    }
    set((s) => ({
      byTab: {
        ...s.byTab,
        [e.tabId]: {
          ...job,
          jobId: e.jobId,
          status: e.cancelled ? 'cancelled' : 'done',
          done: e.cancelled ? job.done : e.results.length,
          total: e.cancelled ? job.total : e.results.length,
          currentPath: null,
          results: e.results,
        },
      },
    }))
    summarize(e)
  },

  forgetTab: (tabId, options) => {
    const job = get().byTab[tabId]
    if (!job) return
    if (options?.cancel && job.jobId && isJobActive(job)) void cancelJob(job.jobId).catch(() => undefined)
    set((s) => {
      const byTab = { ...s.byTab }
      delete byTab[tabId]
      return { byTab }
    })
  },
}))

/** Subscribes to backend job events (inside Tauri only). Returns an unsubscribe function. */
export async function initJobEvents(): Promise<() => void> {
  if (!inTauri()) return () => undefined
  const unlisten: UnlistenFn[] = await Promise.all([
    listen<JobProgressEvent>(JOB_PROGRESS_EVENT, (e) => useJobs.getState().handleProgress(e.payload)),
    listen<JobFinishedEvent>(JOB_FINISHED_EVENT, (e) => useJobs.getState().handleFinished(e.payload)),
  ])
  return () => unlisten.forEach((u) => u())
}

export function useTabJob(tabId: string): JobState | undefined {
  return useJobs((s) => s.byTab[tabId])
}
