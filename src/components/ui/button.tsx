import { cva, type VariantProps } from 'class-variance-authority'
import { forwardRef, type ButtonHTMLAttributes } from 'react'

import { cn } from '@/lib/utils'

export const buttonVariants = cva(
  'inline-flex shrink-0 cursor-default items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-[color,background-color,border-color,box-shadow,opacity] outline-none focus-visible:ring-2 focus-visible:ring-ring/60 focus-visible:ring-offset-1 focus-visible:ring-offset-background disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:shrink-0 [&_svg:not([class*=size-])]:size-4',
  {
    variants: {
      variant: {
        default: 'bg-primary text-primary-foreground shadow-sm hover:bg-primary/90 active:bg-primary/85',
        destructive: 'bg-destructive text-white shadow-sm hover:bg-destructive/90',
        outline: 'border border-border bg-card shadow-xs hover:bg-accent hover:text-foreground',
        secondary: 'bg-muted text-foreground hover:bg-muted/70',
        ghost: 'text-foreground/80 hover:bg-accent hover:text-foreground',
        link: 'text-primary underline-offset-4 hover:underline',
      },
      size: {
        default: 'h-9 px-4',
        sm: 'h-8 gap-1.5 px-3 text-[13px]',
        xs: 'h-7 gap-1 rounded-sm px-2 text-xs',
        lg: 'h-10 px-6',
        icon: 'size-9',
        'icon-sm': 'size-8',
        'icon-xs': 'size-6 rounded-sm',
      },
    },
    defaultVariants: { variant: 'default', size: 'default' },
  },
)

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof buttonVariants>

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant, size, type = 'button', ...props },
  ref,
) {
  return <button ref={ref} type={type} className={cn(buttonVariants({ variant, size }), className)} {...props} />
})
