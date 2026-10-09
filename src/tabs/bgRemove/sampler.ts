// Full-resolution pixel sampler for the eyedropper: the original image is
// drawn once into an offscreen canvas and single pixels are read back.
import type { Rgba } from './schema'

export interface PixelSampler {
  width: number
  height: number
  /** RGBA at integer pixel coordinates; null outside the image. */
  sample: (x: number, y: number) => Rgba | null
  dispose: () => void
}

export async function createPixelSampler(source: Blob): Promise<PixelSampler> {
  const bitmap = await createImageBitmap(source)
  const { width, height } = bitmap
  const canvas = document.createElement('canvas')
  canvas.width = width
  canvas.height = height
  const ctx = canvas.getContext('2d', { willReadFrequently: true })
  if (!ctx) {
    bitmap.close?.()
    throw new Error('2D canvas is not available')
  }
  ctx.drawImage(bitmap, 0, 0)
  bitmap.close?.()
  return {
    width,
    height,
    sample: (x, y) => {
      const px = Math.floor(x)
      const py = Math.floor(y)
      if (px < 0 || py < 0 || px >= width || py >= height) return null
      const d = ctx.getImageData(px, py, 1, 1).data
      return [d[0], d[1], d[2], d[3]]
    },
    dispose: () => {
      canvas.width = 0
      canvas.height = 0
    },
  }
}
