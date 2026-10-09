import { cva, type VariantProps } from 'class-variance-authority'
import { LoaderCircleIcon } from 'lucide-react'
import type { ComponentProps } from 'react'

import { cn } from '@/lib/utils'

export function Progress({ value, className, ...props }: ComponentProps<'div'> & { value: number | null }) {
  const pct = value == null ? null : Math.max(0, Math.min(100, value))
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={pct ?? undefined}
      className={cn('relative h-1.5 w-full overflow-hidden rounded-full bg-muted', className)}
      {...props}
    >
      <div
        className={cn('h-full rounded-full bg-primary transition-[width] duration-300', pct == null && 'w-1/3 animate-pulse')}
        style={pct == null ? undefined : { width: `${pct}%` }}
      />
    </div>
  )
}

export const badgeVariants = cva(
  'inline-flex shrink-0 items-center gap-1 whitespace-nowrap rounded-full border px-1.5 py-px text-[10px] font-semibold leading-4 tabular-nums [&_svg]:size-3',
  {
    variants: {
      variant: {
        default: 'border-transparent bg-primary text-primary-foreground',
        secondary: 'border-transparent bg-muted text-muted-foreground',
        outline: 'border-border text-foreground',
        warning: 'border-transparent bg-amber-500/90 text-white',
        destructive: 'border-transparent bg-destructive text-white',
      },
    },
    defaultVariants: { variant: 'default' },
  },
)

export function Badge({ className, variant, ...props }: ComponentProps<'span'> & VariantProps<typeof badgeVariants>) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />
}

export function Spinner({ className, ...props }: ComponentProps<typeof LoaderCircleIcon>) {
  return <LoaderCircleIcon aria-hidden className={cn('size-4 animate-spin', className)} {...props} />
}

export function Label({ className, ...props }: ComponentProps<'label'>) {
  return <label className={cn('text-[13px] font-medium leading-none text-foreground/90', className)} {...props} />
}

export function Separator({ className, orientation = 'horizontal', ...props }: ComponentProps<'div'> & { orientation?: 'horizontal' | 'vertical' }) {
  return (
    <div
      role="separator"
      aria-orientation={orientation}
      className={cn('shrink-0 bg-border', orientation === 'horizontal' ? 'h-px w-full' : 'h-5 w-px', className)}
      {...props}
    />
  )
}

export function Kbd({ className, ...props }: ComponentProps<'kbd'>) {
  return (
    <kbd
      className={cn('rounded border border-border bg-muted px-1.5 py-px font-sans text-[10px] font-medium text-muted-foreground', className)}
      {...props}
    />
  )
}
