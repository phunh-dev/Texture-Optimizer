import { describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
  convertFileSrc: (path: string, protocol: string) => `${protocol}://localhost/${encodeURIComponent(path)}`,
}))

import { invoke } from '@tauri-apps/api/core'

import { decodePreviewPayload, previewOp, thumbnailUrl } from './index'

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

  it('thumbnailUrl encodes the path and adds size + mtime', () => {
    expect(thumbnailUrl({ path: '/x y/ä.png', mtimeMs: 42 }, 'small')).toBe(
      'thumb://localhost/%2Fx%20y%2F%C3%A4.png?size=small&m=42',
    )
  })
})
