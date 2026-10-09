import { ChevronDownIcon, ChevronUpIcon } from 'lucide-react'
import { useEffect, useRef, useState, type InputHTMLAttributes, type KeyboardEvent } from 'react'

import { clamp, cn } from '@/lib/utils'

import { inputClass } from './input'

export interface NumberInputProps
  extends Omit<InputHTMLAttributes<HTMLInputElement>, 'value' | 'onChange' | 'type' | 'min' | 'max' | 'step'> {
  value: number
  /** Called for every valid number typed or stepped. */
  onValueChange: (value: number) => void
  /** Called when editing ends (blur / Enter / step button release). */
  onCommit?: () => void
  min?: number
  max?: number
  step?: number
  /** Already translated unit suffix, e.g. "px". */
  unit?: string
  /** Hide the up/down stepper buttons. */
  hideSteppers?: boolean
}

function decimalsOf(step: number): number {
  const s = String(step)
  const dot = s.indexOf('.')
  return dot < 0 ? 0 : s.length - dot - 1
}

/** Numeric text field: free typing (no caret jumps), arrow keys step (Shift ×10). */
export function NumberInput({
  value,
  onValueChange,
  onCommit,
  min,
  max,
  step = 1,
  unit,
  hideSteppers,
  className,
  onBlur,
  onKeyDown,
  ...props
}: NumberInputProps) {
  const [draft, setDraft] = useState(() => String(value))
  const focused = useRef(false)

  // Sync from outside (undo, presets) unless the user is typing.
  useEffect(() => {
    if (!focused.current) setDraft(Number.isFinite(value) ? String(value) : '')
  }, [value])

  const stepBy = (delta: number) => {
    const base = Number.isFinite(value) ? value : (min ?? 0)
    const next = Number((base + delta).toFixed(decimalsOf(step)))
    const bounded = clamp(next, min ?? -Infinity, max ?? Infinity)
    setDraft(String(bounded))
    onValueChange(bounded)
  }

  const handleKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    onKeyDown?.(e)
    if (e.defaultPrevented) return
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault()
      stepBy((e.key === 'ArrowUp' ? 1 : -1) * step * (e.shiftKey ? 10 : 1))
    } else if (e.key === 'Enter') {
      setDraft(String(value))
      onCommit?.()
    }
  }

  return (
    <div className={cn('group/num relative flex items-center', className)}>
      <input
        {...props}
        type="text"
        inputMode="decimal"
        role="spinbutton"
        aria-valuenow={Number.isFinite(value) ? value : undefined}
        aria-valuemin={min}
        aria-valuemax={max}
        value={draft}
        className={cn(inputClass, 'h-8 tabular-nums', unit ? 'pr-12' : 'pr-7', hideSteppers && (unit ? 'pr-9' : 'pr-3'))}
        onFocus={() => {
          focused.current = true
        }}
        onChange={(e) => {
          const text = e.target.value
          setDraft(text)
          const n = Number(text.replace(',', '.'))
          if (text.trim() !== '' && Number.isFinite(n)) onValueChange(n)
        }}
        onBlur={(e) => {
          focused.current = false
          setDraft(String(value))
          onCommit?.()
          onBlur?.(e)
        }}
        onKeyDown={handleKeyDown}
      />
      {unit ? (
        <span className={cn('pointer-events-none absolute text-xs text-muted-foreground', hideSteppers ? 'right-3' : 'right-7')}>{unit}</span>
      ) : null}
      {hideSteppers ? null : (
        <div className="absolute right-1 flex flex-col opacity-60 transition-opacity group-hover/num:opacity-100">
          <button
            type="button"
            tabIndex={-1}
            aria-hidden
            className="flex h-3.5 w-4 items-center justify-center rounded-sm hover:bg-accent"
            onClick={() => {
              stepBy(step)
              onCommit?.()
            }}
          >
            <ChevronUpIcon className="size-3" />
          </button>
          <button
            type="button"
            tabIndex={-1}
            aria-hidden
            className="flex h-3.5 w-4 items-center justify-center rounded-sm hover:bg-accent"
            onClick={() => {
              stepBy(-step)
              onCommit?.()
            }}
          >
            <ChevronDownIcon className="size-3" />
          </button>
        </div>
      )}
    </div>
  )
}
