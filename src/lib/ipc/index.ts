// Typed wrappers around Tauri commands. Rust command names are snake_case
// versions of these function names (e.g. scanPaths -> scan_paths).
import { convertFileSrc, invoke } from '@tauri-apps/api/core'

import type {
  ImportedFile,
  OpRequest,
  OutputSettings,
  PreviewResult,
  ScanOptions,
  ScanResult,
  ThumbSize,
} from './types'

export * from './types'

/** Expand files/folders into supported images with header info. */
export function scanPaths(paths: string[], options: ScanOptions): Promise<ScanResult> {
  return invoke('scan_paths', { paths, options })
}

/** URL served by the `thumb` custom protocol (disk-cached WebP thumbnail). */
export function thumbnailUrl(file: Pick<ImportedFile, 'path' | 'mtimeMs'>, size: ThumbSize): string {
  return `${convertFileSrc(file.path, 'thumb')}?size=${size}&m=${file.mtimeMs}`
}

/** Process one image in memory and return the encoded result for preview. */
export async function previewOp(tabId: string, path: string, request: OpRequest): Promise<PreviewResult> {
  return invoke('preview_op', { tabId, path, request })
}

/** Start a batch job; progress arrives via JOB_PROGRESS_EVENT / JOB_FINISHED_EVENT. Returns the job id. */
export function runOp(tabId: string, request: OpRequest, paths: string[], output: OutputSettings): Promise<string> {
  return invoke('run_op', { tabId, request, paths, output })
}

export function cancelJob(jobId: string): Promise<void> {
  return invoke('cancel_job', { jobId })
}

/** Called when a tab goes to sleep: backend drops decoded image caches for it. */
export function releaseSession(tabId: string): Promise<void> {
  return invoke('release_session', { tabId })
}
