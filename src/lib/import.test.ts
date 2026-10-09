import { beforeEach, describe, expect, it, vi } from 'vitest'

import ioRs from '../../crates/texopt-core/src/io.rs?raw'

import { scanPaths } from '@/lib/ipc'
import { resetSessions } from '@/stores/session'

import { getTabImporter, IMAGE_EXTENSIONS, importPaths, registerTabImporter, type TabImporter } from './import'

vi.mock('@/lib/ipc', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@/lib/ipc')>()),
  scanPaths: vi.fn(() => Promise.resolve({ files: [], skipped: [] })),
}))

describe('tab importers', () => {
  beforeEach(() => {
    resetSessions()
    vi.mocked(scanPaths).mockClear()
  })

  it('a registered importer replaces the image scan for its tab only', async () => {
    const importer: TabImporter = {
      importPaths: vi.fn(() => Promise.resolve({ added: 1, duplicates: 0, skipped: 0 })),
      filterNameKey: 'mesh:toast.modelsFilter',
      extensions: ['fbx'],
    }
    const unregister = registerTabImporter('t1', importer)
    expect(getTabImporter('t1')).toBe(importer)
    expect(await importPaths('t1', ['C:/a.fbx'], { recursive: true })).toEqual({ added: 1, duplicates: 0, skipped: 0 })
    expect(importer.importPaths).toHaveBeenCalledWith('t1', ['C:/a.fbx'], { recursive: true })
    expect(scanPaths).not.toHaveBeenCalled()

    await importPaths('t2', ['C:/b.png'], { recursive: false })
    expect(scanPaths).toHaveBeenCalledWith(['C:/b.png'], { recursive: false })

    unregister()
    expect(getTabImporter('t1')).toBeUndefined()
    await importPaths('t1', ['C:/c.png'], { recursive: false })
    expect(scanPaths).toHaveBeenCalledTimes(2)
  })

  it('unregistering a replaced importer keeps the newer one', () => {
    const a: TabImporter = { importPaths: vi.fn(), filterNameKey: 'x', extensions: [] }
    const b: TabImporter = { importPaths: vi.fn(), filterNameKey: 'y', extensions: [] }
    const offA = registerTabImporter('t3', a)
    registerTabImporter('t3', b)
    offA()
    expect(getTabImporter('t3')).toBe(b)
  })
})

describe('IMAGE_EXTENSIONS', () => {
  it('matches the formats the Rust backend can decode', () => {
    const rust = /SUPPORTED_EXTENSIONS: &\[&str\] = &\[([^\]]*)\]/.exec(ioRs)?.[1]
    expect(rust).toBeDefined()
    const list = [...rust!.matchAll(/"([a-z0-9]+)"/g)].map((m) => m[1])
    expect([...IMAGE_EXTENSIONS]).toEqual(list)
  })
})
