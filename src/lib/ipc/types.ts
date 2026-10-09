// IPC contract between the React app and the Rust backend.
// Rust structs mirror these shapes with #[serde(rename_all = "camelCase")].
// Change both sides together.

/** Translatable error returned by every command: UI shows t(`errors:${code}`, params). */
export interface AppError {
  code: string
  params: Record<string, unknown>
}

/** An image discovered by `scanPaths` (dimensions read from the header only). */
export interface ImportedFile {
  /** Stable id: blake3 of the absolute path. */
  id: string
  path: string
  /** File name with extension. */
  name: string
  /** Lowercase extension without dot. */
  ext: string
  width: number
  height: number
  sizeBytes: number
  mtimeMs: number
}

export interface ScanOptions {
  /** Descend into sub-folders when a folder path is given. */
  recursive: boolean
  /**
   * Extensions to accept when expanding folders (case-insensitive, leading dot optional);
   * defaults to every supported format. Explicitly listed files are not filtered.
   */
  extensions?: string[]
}

export interface ScanResult {
  files: ImportedFile[]
  /** Paths that were skipped (unsupported or unreadable) with the reason. */
  skipped: { path: string; error: AppError }[]
}

export type ThumbSize = 'small' | 'medium' | 'large'

/** Single-image operation request; `params` shape is defined per feature tab. */
export interface OpRequest {
  kind: 'resize' | 'resolution' | 'potPad' | 'trim' | 'bgRemove'
  params: Record<string, unknown>
}

export type OutputFormat = 'keep' | 'png' | 'tga' | 'jpg' | 'webp'

/** What to do when a file being saved already exists. autoRename picks `name_1.ext`, `name_2.ext`, ... */
export type ConflictPolicy = 'overwrite' | 'skip' | 'autoRename'

/**
 * Encoding settings of a run. There is no destination: `runOp` always processes into the tab's
 * staging folder and the user saves the results afterwards (`saveResults`), where `conflict` applies.
 */
export interface OutputSettings {
  format: OutputFormat
  /** PNG zlib effort. */
  pngCompression: 'fast' | 'default' | 'best'
  /** 1-100, JPEG only (WebP output is always lossless; JPEG flattens alpha onto white). */
  jpgQuality: number
  /** Run a lossless PNG optimizer pass after encoding. */
  optimizePng: boolean
  /** Applied when saving the results to a folder. */
  conflict: ConflictPolicy
  /** Stage the op metadata (e.g. trim offsets) as `<output>.json` next to each result. Optional, default false. */
  writeMeta?: boolean
}

/** A staged result image of a run (`listResults`). */
export interface StagedResult {
  /** Header info of the staged image; `path` is inside the staging folder. */
  file: ImportedFile
  /** Its metadata sidecar (`<image>.json`), if any. */
  sidecar: string | null
}

/**
 * Where `saveResults` copies the staged results: every image into a folder (conflict policy
 * applies), or "Save As" of the single staged image to exactly `path` (another image extension
 * re-encodes to that format).
 */
export type SaveTarget = { kind: 'folder'; path: string } | { kind: 'file'; path: string }

export interface SavedFile {
  from: string
  to: string
  sidecar: string | null
}

export interface SaveReport {
  /** Folder the files were saved to. */
  destination: string
  saved: SavedFile[]
  /** Targets that exist and were left alone (policy `skip`). */
  skipped: string[]
  failed: { path: string; error: AppError }[]
}

export interface PreviewResult {
  /** Encoded PNG of the processed image. */
  png: ArrayBuffer
  width: number
  height: number
  meta: unknown | null
}

export interface JobProgressEvent {
  jobId: string
  tabId: string
  done: number
  total: number
  currentPath: string | null
}

export interface JobFileResult {
  input: string
  output: string | null
  error: AppError | null
  meta: unknown | null
}

export interface JobFinishedEvent {
  jobId: string
  tabId: string
  cancelled: boolean
  results: JobFileResult[]
}

export const JOB_PROGRESS_EVENT = 'job://progress'
export const JOB_FINISHED_EVENT = 'job://finished'
