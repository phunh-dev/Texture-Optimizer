import * as ToggleGroupPrimitive from '@radix-ui/react-toggle-group'
import type { ComponentProps } from 'react'

import { cn } from '@/lib/utils'

/** Segmented control look: a muted track with a raised selected item. */
export function ToggleGroup({ className, ...props }: ComponentProps<typeof ToggleGroupPrimitive.Root>) {
  return (
    <ToggleGroupPrimitive.Root
      className={cn('inline-flex items-center gap-0.5 rounded-lg bg-muted p-0.5', className)}
      {...props}
    />
  )
}

export function ToggleGroupItem({ className, ...props }: ComponentProps<typeof ToggleGroupPrimitive.Item>) {
  return (
    <ToggleGroupPrimitive.Item
      className={cn(
        'inline-flex h-7 min-w-7 flex-1 cursor-default items-center justify-center gap-1.5 whitespace-nowrap rounded-md px-2 text-xs font-medium text-muted-foreground outline-none transition-all',
        'hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50 disabled:opacity-50',
        'data-[state=on]:bg-card data-[state=on]:text-foreground data-[state=on]:shadow-sm [&_svg]:size-3.5',
        className,
      )}
      {...props}
    />
  )
}
