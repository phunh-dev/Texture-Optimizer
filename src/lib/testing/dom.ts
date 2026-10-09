// Test-only helpers: jsdom has no layout, ResizeObserver or pointer capture.
// Imported by *.test.tsx files only.
import type { ImportedFile } from '@/lib/ipc/types'

export function installDomMocks({ width = 1000, height = 800 }: { width?: number; height?: number } = {}): void {
  const define = (prop: string, value: number) =>
    Object.defineProperty(HTMLElement.prototype, prop, { configurable: true, get: () => value })
  define('clientWidth', width)
  define('clientHeight', height)
  define('offsetWidth', width)
  define('offsetHeight', height)

  class ResizeObserverMock {
    private readonly cb: ResizeObserverCallback
    constructor(cb: ResizeObserverCallback) {
      this.cb = cb
    }
    observe(target: Element) {
      this.cb([{ target, contentRect: { width, height } } as unknown as ResizeObserverEntry], this as unknown as ResizeObserver)
    }
    unobserve() {}
    disconnect() {}
  }
  globalThis.ResizeObserver = ResizeObserverMock as unknown as typeof ResizeObserver

  const proto = Element.prototype as unknown as Record<string, unknown>
  proto.scrollIntoView = () => undefined
  proto.hasPointerCapture = () => false
  proto.setPointerCapture = () => undefined
  proto.releasePointerCapture = () => undefined
  if (!window.matchMedia) {
    window.matchMedia = ((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      dispatchEvent: () => false,
    })) as unknown as typeof window.matchMedia
  }
}

let counter = 0
export function makeFile(overrides: Partial<ImportedFile> = {}): ImportedFile {
  counter++
  const name = overrides.name ?? `image_${counter}.png`
  return {
    id: overrides.id ?? `id-${counter}`,
    path: overrides.path ?? `C:/textures/${name}`,
    name,
    ext: 'png',
    width: 256,
    height: 256,
    sizeBytes: 1024,
    mtimeMs: 1,
    ...overrides,
  }
}

export function makeFiles(n: number, overrides: Partial<ImportedFile> = {}): ImportedFile[] {
  return Array.from({ length: n }, (_, i) => makeFile({ id: `f${i}`, name: `tex_${i}.png`, ...overrides }))
}
