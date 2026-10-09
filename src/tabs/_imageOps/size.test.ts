// Replays the tables of crates/texopt-core/tests/ops_common.rs against the TS mirror.
import { describe, expect, it } from 'vitest'

import { hexToRgba, rgbaToHex } from './color'
import { anchorOffset, isPot, nextPot, prevPot, roundPot, roundToMultiple, scaleDimension, snapDimension } from './size'

describe('size math mirrors ops::common', () => {
  it('pot predicates and tables', () => {
    for (const n of [1, 2, 4, 8, 1024, 2 ** 31]) expect(isPot(n), `${n}`).toBe(true)
    for (const n of [0, 3, 6, 1023, 1025]) expect(isPot(n), `${n}`).toBe(false)
    const next: [number, number][] = [
      [0, 1],
      [1, 1],
      [2, 2],
      [3, 4],
      [1023, 1024],
      [1024, 1024],
      [1025, 2048],
      [4097, 8192],
    ]
    for (const [n, e] of next) expect(nextPot(n), `nextPot(${n})`).toBe(e)
    const prev: [number, number][] = [
      [0, 1],
      [1, 1],
      [2, 2],
      [3, 2],
      [1023, 512],
      [1024, 1024],
      [1025, 1024],
      [4097, 4096],
    ]
    for (const [n, e] of prev) expect(prevPot(n), `prevPot(${n})`).toBe(e)
    const nearest: [number, number][] = [
      [1, 1],
      [3, 4],
      [5, 4],
      [6, 8],
      [1023, 1024],
      [1025, 1024],
      [1536, 2048],
      [4097, 4096],
    ]
    for (const [n, e] of nearest) expect(roundPot(n, 'nearest'), `roundPot(${n})`).toBe(e)
    expect(roundPot(1025, 'up')).toBe(2048)
    expect(roundPot(1025, 'down')).toBe(1024)
    expect(roundPot(1, 'down')).toBe(1)
  })

  it('round_to_multiple tables', () => {
    // (n, m, nearest, up, down)
    const cases: [number, number, number, number, number][] = [
      [0, 4, 4, 4, 4],
      [1, 4, 4, 4, 4],
      [3, 4, 4, 4, 4],
      [4, 4, 4, 4, 4],
      [6, 4, 8, 8, 4],
      [1023, 4, 1024, 1024, 1020],
      [1025, 4, 1024, 1028, 1024],
      [4097, 4, 4096, 4100, 4096],
      [15, 10, 20, 20, 10],
      [1023, 10, 1020, 1030, 1020],
    ]
    for (const [n, m, near, up, down] of cases) {
      expect(roundToMultiple(n, m, 'nearest'), `nearest ${n}/${m}`).toBe(near)
      expect(roundToMultiple(n, m, 'up'), `up ${n}/${m}`).toBe(up)
      expect(roundToMultiple(n, m, 'down'), `down ${n}/${m}`).toBe(down)
    }
  })

  it('snap modes', () => {
    const snap = (w: number, h: number, s: 'none' | 'multipleOf4' | 'pot', r: 'nearest' | 'up') => [snapDimension(w, s, r), snapDimension(h, s, r)]
    expect(snap(50, 25, 'none', 'nearest')).toEqual([50, 25])
    expect(snap(50, 25, 'multipleOf4', 'nearest')).toEqual([52, 24])
    expect(snap(50, 25, 'multipleOf4', 'up')).toEqual([52, 28])
    expect(snap(50, 25, 'pot', 'nearest')).toEqual([64, 32])
    expect(snap(50, 25, 'pot', 'up')).toEqual([64, 32])
    expect(snap(50, 17, 'pot', 'nearest')).toEqual([64, 16])
  })

  it('scale_dimension rounds half up, never below 1', () => {
    expect(scaleDimension(10, 0.333)).toBe(3)
    expect(scaleDimension(5, 0.5)).toBe(3)
    expect(scaleDimension(10, 0.01)).toBe(1)
    expect(scaleDimension(100, 0.5)).toBe(50)
  })

  it('anchor offsets: pad and crop', () => {
    const expected: [Parameters<typeof anchorOffset>[0], [number, number]][] = [
      ['topLeft', [0, 0]],
      ['top', [3, 0]],
      ['topRight', [6, 0]],
      ['left', [0, 3]],
      ['center', [3, 3]],
      ['right', [6, 3]],
      ['bottomLeft', [0, 6]],
      ['bottom', [3, 6]],
      ['bottomRight', [6, 6]],
    ]
    for (const [a, [x, y]] of expected) {
      expect(anchorOffset(a, 10, 10, 4, 4), `pad ${a}`).toEqual([x, y])
      const [cx, cy] = anchorOffset(a, 4, 4, 10, 10)
      expect([cx + 0, cy + 0], `crop ${a}`).toEqual([-x + 0, -y + 0]) // +0 normalizes -0
    }
    // Odd remainders truncate toward zero (content biased to the top-left).
    expect(anchorOffset('center', 5, 5, 2, 2)).toEqual([1, 1])
    expect(anchorOffset('center', 2, 2, 5, 5)).toEqual([-1, -1])
  })
})

describe('color conversion', () => {
  it('hex strings map to Rust [r, g, b, a] and back', () => {
    expect(hexToRgba('#00000000')).toEqual([0, 0, 0, 0])
    expect(hexToRgba('#ff8000')).toEqual([255, 128, 0, 255])
    expect(hexToRgba('#0102037F')).toEqual([1, 2, 3, 127])
    expect(hexToRgba('nope', [9, 9, 9, 9])).toEqual([9, 9, 9, 9])
    expect(rgbaToHex([255, 128, 0, 255])).toBe('#ff8000ff')
  })
})
