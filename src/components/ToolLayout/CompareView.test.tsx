import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { afterEach, beforeAll, describe, expect, it } from 'vitest'

import { TooltipProvider } from '@/components/ui/tooltip'
import { installDomMocks } from '@/lib/testing/dom'

import { CompareView } from './CompareView'

describe('CompareView placement props', () => {
  beforeAll(() => installDomMocks({ width: 1000, height: 800 }))
  afterEach(cleanup)

  it('keeps the default layering when no placement is given', () => {
    const { container } = render(
      <TooltipProvider>
        <CompareView before="a" after="b" />
      </TooltipProvider>,
    )
    const [before, after] = container.querySelectorAll('img')
    expect(before.style.width).toBe('')
    expect(after.style.transform).toBe(before.style.transform)
    expect(screen.queryByTestId('compare-before-label')).not.toBeInTheDocument()
  })

  it('shows labels, sizes the before layer and positions the after layer in before pixels', () => {
    const { container } = render(
      <TooltipProvider>
        <CompareView
          before="a"
          after="b"
          beforeLabel="100×50"
          afterLabel="128×64"
          beforeSize={{ width: 100, height: 50 }}
          afterRect={{ x: -14, y: -7, width: 128, height: 64 }}
        />
      </TooltipProvider>,
    )
    const [before, after] = container.querySelectorAll('img')
    expect(screen.getByTestId('compare-before-label')).toHaveTextContent('100×50')
    expect(screen.getByTestId('compare-after-label')).toHaveTextContent('128×64')
    expect(before.style.width).toBe('100px')
    expect(before.style.height).toBe('50px')
    expect(after.style.width).toBe('128px')
    expect(after.style.height).toBe('64px')
    expect(after.style.transform).toBe(`${before.style.transform} translate(-14px, -7px)`)

    // Fit uses the union of both layers (128×64 box starting at -14,-7): zoom = min(952/128, 752/64).
    fireEvent.load(before)
    expect(screen.getByText('744%')).toBeInTheDocument()
    expect(before.style.transform).toBe(`translate(${(1000 - 128 * 7.4375) / 2 + 14 * 7.4375}px, ${(800 - 64 * 7.4375) / 2 + 7 * 7.4375}px) scale(7.4375)`)
  })
})
