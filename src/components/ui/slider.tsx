import * as SliderPrimitive from '@radix-ui/react-slider'
import type { ComponentProps } from 'react'

import { cn } from '@/lib/utils'

export function Slider({ className, value, defaultValue, ...props }: ComponentProps<typeof SliderPrimitive.Root>) {
  const thumbs = (value ?? defaultValue ?? [0]).length
  return (
    <SliderPrimitive.Root
      value={value}
      defaultValue={defaultValue}
      className={cn('relative flex h-5 w-full touch-none select-none items-center data-[disabled]:opacity-50', className)}
      {...props}
    >
      <SliderPrimitive.Track className="relative h-1.5 w-full grow overflow-hidden rounded-full bg-muted">
        <SliderPrimitive.Range className="absolute h-full bg-primary" />
      </SliderPrimitive.Track>
      {Array.from({ length: thumbs }, (_, i) => (
        <SliderPrimitive.Thumb
          key={i}
          className="block size-4 rounded-full border-2 border-primary bg-card shadow-sm transition-[box-shadow] outline-none hover:ring-4 hover:ring-ring/20 focus-visible:ring-4 focus-visible:ring-ring/40"
          aria-label={props['aria-label']}
        />
      ))}
    </SliderPrimitive.Root>
  )
}
