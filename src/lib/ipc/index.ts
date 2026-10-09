// Typed wrappers around Tauri commands. Rust command names are snake_case
// versions of these function names (e.g. scanPaths -> scan_paths).
import { convertFileSrc, invoke } from '@tauri-apps/api/core'

import type {
  ConflictPolicy,
  ImportedFile,
  OpRequest,
  OutputSettings,
  PreviewResult,
  SaveReport,
  SaveTarget,
  ScanOptions,
  ScanResult,
  StagedResult,
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

/**
 * URL of the full-resolution original served by the `thumb` protocol
 * (`size=full`): displayable formats byte-for-byte, others (TGA) as PNG.
 * Used as the "before" image of previews.
 */
export function originalImageUrl(file: Pick<ImportedFile, 'path' | 'mtimeMs'>): string {
  return `${convertFileSrc(file.path, 'thumb')}?size=full&m=${file.mtimeMs}`
}

/**
 * Decode the binary `preview_op` payload (see `src-tauri/src/preview.rs`):
 * little-endian u32 `width | height | metaLen`, then `metaLen` bytes of UTF-8
 * JSON (0 = null), then the PNG bytes.
 */
export function decodePreviewPayload(payload: ArrayBuffer | Uint8Array | number[]): PreviewResult {
  const bytes =
    payload instanceof Uint8Array
      ? payload
      : Array.isArray(payload)
        ? Uint8Array.from(payload)
        : new Uint8Array(payload)
  const HEADER = 12
  if (bytes.byteLength < HEADER) throw new Error('preview payload too short')
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength)
  const width = view.getUint32(0, true)
  const height = view.getUint32(4, true)
  const metaLen = view.getUint32(8, true)
  const meta: unknown =
    metaLen > 0 ? JSON.parse(new TextDecoder().decode(bytes.subarray(HEADER, HEADER + metaLen))) : null
  const start = bytes.byteOffset + HEADER + metaLen
  const png = bytes.buffer.slice(start, bytes.byteOffset + bytes.byteLength) as ArrayBuffer
  return { png, width, height, meta }
}

/** Process one image in memory and return the encoded result for preview. */
export async function previewOp(tabId: string, path: string, request: OpRequest): Promise<PreviewResult> {
  const payload = await invoke<ArrayBuffer | number[]>('preview_op', { tabId, path, request })
  return decodePreviewPayload(payload)
}

/** Encoding settings only: a legacy destination `mode` (in place / suffix / folder) is never sent. */
export function encodingSettings(output: OutputSettings): OutputSettings {
  const settings = { ...output } as OutputSettings & { mode?: unknown }
  delete settings.mode
  return settings
}

/**
 * Start a batch job; progress arrives via JOB_PROGRESS_EVENT / JOB_FINISHED_EVENT. Returns the job id.
 * Results are only staged (`<appCacheDir>/staging/<tabId>/<jobId>/`), never written next to the
 * sources: the user saves them with `saveResults`.
 */
export function runOp(tabId: string, request: OpRequest, paths: string[], output: OutputSettings): Promise<string> {
  return invoke('run_op', { tabId, request, paths, output: encodingSettings(output) })
}

/** Header info of the staged results of a job (naturally sorted by name). */
export function listResults(tabId: string, jobId: string): Promise<StagedResult[]> {
  return invoke('list_results', { tabId, jobId })
}

/**
 * Copy the staged results of a job (and their sidecars) to a folder, applying `conflict`, or the
 * single result to an exact "Save As" path. `output` gives the encoding when Save As changes the format.
 */
export function saveResults(
  tabId: string,
  jobId: string,
  target: SaveTarget,
  conflict: ConflictPolicy,
  output: OutputSettings,
): Promise<SaveReport> {
  return invoke('save_results', { tabId, jobId, target, conflict, output: encodingSettings(output) })
}

/** Delete the staged results of a tab (Discard, tab closed). */
export function discardResults(tabId: string): Promise<void> {
  return invoke('discard_results', { tabId })
}

export function cancelJob(jobId: string): Promise<void> {
  return invoke('cancel_job', { jobId })
}

/** Called when a tab goes to sleep: backend drops decoded image caches for it. */
export function releaseSession(tabId: string): Promise<void> {
  return invoke('release_session', { tabId })
}
