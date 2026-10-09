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

export type OutputMode =
  | { kind: 'inPlace' }
  | { kind: 'folder'; path: string }
  | { kind: 'suffix'; suffix: string }

export type OutputFormat = 'keep' | 'png' | 'tga' | 'jpg' | 'webp'

export interface OutputSettings {
  mode: OutputMode
  format: OutputFormat
  /** PNG zlib effort. */
  pngCompression: 'fast' | 'default' | 'best'
  /** 1-100, JPEG only (WebP output is always lossless; JPEG flattens alpha onto white). */
  jpgQuality: number
  /** Run a lossless PNG optimizer pass after encoding. */
  optimizePng: boolean
  /**
   * What to do when the target file already exists. Never applied to the input itself, so
   * inPlace + format 'keep' always overwrites the source. autoRename picks `name_1.ext`, `name_2.ext`, ...
   */
  conflict: 'overwrite' | 'skip' | 'autoRename'
  /** Write the op metadata (e.g. trim offsets) to `<output>.json`. Optional, default false. */
  writeMeta?: boolean
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
