import { useLayoutEffect, useState, type RefObject } from 'react'

export interface Size {
  width: number
  height: number
}

/** Tracks an element's content box size with ResizeObserver (falls back to window resize). */
export function useElementSize(ref: RefObject<HTMLElement | null>): Size {
  const [size, setSize] = useState<Size>({ width: 0, height: 0 })

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const update = (width: number, height: number) =>
      setSize((prev) => (prev.width === width && prev.height === height ? prev : { width, height }))
    update(el.clientWidth, el.clientHeight)

    if (typeof ResizeObserver === 'undefined') {
      const onResize = () => update(el.clientWidth, el.clientHeight)
      window.addEventListener('resize', onResize)
      return () => window.removeEventListener('resize', onResize)
    }
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect
      if (rect) update(Math.round(rect.width), Math.round(rect.height))
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [ref])

  return size
}
