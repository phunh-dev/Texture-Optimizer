import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${encodeURIComponent(path)}`,
}))

import { invoke } from '@tauri-apps/api/core'

import {
  decodePreviewPayload,
  discardResults,
  listResults,
  originalImageUrl,
  previewOp,
  runOp,
  saveResults,
  thumbnailUrl,
  type OutputSettings,
} from './index'

function payload(width: number, height: number, meta: unknown, png: number[]): Uint8Array {
  const metaBytes = meta === null ? new Uint8Array() : new TextEncoder().encode(JSON.stringify(meta))
  const out = new Uint8Array(12 + metaBytes.length + png.length)
  const view = new DataView(out.buffer)
  view.setUint32(0, width, true)
  view.setUint32(4, height, true)
  view.setUint32(8, metaBytes.length, true)
  out.set(metaBytes, 12)
  out.set(png, 12 + metaBytes.length)
  return out
}

describe('decodePreviewPayload', () => {
  it('decodes size, meta and png bytes', () => {
    const bytes = payload(640, 480, { offsetX: 3, name: 'ảnh' }, [0x89, 0x50, 0x4e, 0x47])
    const r = decodePreviewPayload(bytes.buffer as ArrayBuffer)
    expect(r.width).toBe(640)
    expect(r.height).toBe(480)
    expect(r.meta).toEqual({ offsetX: 3, name: 'ảnh' })
    expect(Array.from(new Uint8Array(r.png))).toEqual([0x89, 0x50, 0x4e, 0x47])
  })

  it('maps empty meta to null and accepts number arrays / offset views', () => {
    const bytes = payload(2, 3, null, [1, 2, 3])
    expect(decodePreviewPayload(Array.from(bytes)).meta).toBeNull()
    const padded = new Uint8Array(bytes.length + 5)
    padded.set(bytes, 5)
    const r = decodePreviewPayload(padded.subarray(5))
    expect([r.width, r.height]).toEqual([2, 3])
    expect(Array.from(new Uint8Array(r.png))).toEqual([1, 2, 3])
  })

  it('rejects truncated payloads', () => {
    expect(() => decodePreviewPayload(new Uint8Array(4))).toThrow()
  })
})

describe('ipc wrappers', () => {
  it('previewOp invokes preview_op and decodes the result', async () => {
    vi.mocked(invoke).mockResolvedValueOnce(payload(5, 6, null, [9]).buffer)
    const r = await previewOp('tab-1', '/a.png', { kind: 'resize', params: {} })
    expect(invoke).toHaveBeenCalledWith('preview_op', {
      tabId: 'tab-1',
      path: '/a.png',
      request: { kind: 'resize', params: {} },
    })
    expect([r.width, r.height]).toEqual([5, 6])
  })

  it('runOp sends encoding settings only (a legacy destination mode is dropped)', async () => {
    vi.mocked(invoke).mockResolvedValueOnce('job-1')
    const output = { format: 'png', pngCompression: 'best', jpgQuality: 80, optimizePng: true, conflict: 'skip' } as const
    const legacy = { ...output, mode: { kind: 'inPlace' } } as unknown as OutputSettings
    await expect(runOp('tab-1', { kind: 'resize', params: {} }, ['/a.png'], legacy)).resolves.toBe('job-1')
    expect(invoke).toHaveBeenLastCalledWith('run_op', {
      tabId: 'tab-1',
      request: { kind: 'resize', params: {} },
      paths: ['/a.png'],
      output,
    })
    expect(legacy).toHaveProperty('mode')
  })

  it('staging commands: listResults, saveResults, discardResults', async () => {
    const output: OutputSettings = { format: 'keep', pngCompression: 'default', jpgQuality: 90, optimizePng: false, conflict: 'autoRename' }
    vi.mocked(invoke).mockResolvedValueOnce([])
    await listResults('tab-1', 'job-2')
    expect(invoke).toHaveBeenLastCalledWith('list_results', { tabId: 'tab-1', jobId: 'job-2' })
    vi.mocked(invoke).mockResolvedValueOnce({ destination: 'D:/out', saved: [], skipped: [], failed: [] })
    await saveResults('tab-1', 'job-2', { kind: 'folder', path: 'D:/out' }, 'skip', output)
    expect(invoke).toHaveBeenLastCalledWith('save_results', {
      tabId: 'tab-1',
      jobId: 'job-2',
      target: { kind: 'folder', path: 'D:/out' },
      conflict: 'skip',
      output,
    })
    vi.mocked(invoke).mockResolvedValueOnce(undefined)
    await discardResults('tab-1')
    expect(invoke).toHaveBeenLastCalledWith('discard_results', { tabId: 'tab-1' })
  })

  it('thumbnailUrl encodes the path and adds size + mtime', () => {
    expect(thumbnailUrl({ path: '/x y/ä.png', mtimeMs: 42 }, 'small')).toBe(
      'thumb://localhost/%2Fx%20y%2F%C3%A4.png?size=small&m=42',
    )
  })

  it('originalImageUrl asks the thumb protocol for the full-size original', () => {
    expect(originalImageUrl({ path: '/x y/ä.tga', mtimeMs: 7 })).toBe('thumb://localhost/%2Fx%20y%2F%C3%A4.tga?size=full&m=7')
  })
})
