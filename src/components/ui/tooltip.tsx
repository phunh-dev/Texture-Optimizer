import * as TooltipPrimitive from '@radix-ui/react-tooltip'
import type { ComponentProps, ReactNode } from 'react'

import { cn } from '@/lib/utils'

export const TooltipProvider = TooltipPrimitive.Provider
export const TooltipRoot = TooltipPrimitive.Root
export const TooltipTrigger = TooltipPrimitive.Trigger

export function TooltipContent({ className, sideOffset = 6, children, ...props }: ComponentProps<typeof TooltipPrimitive.Content>) {
  return (
    <TooltipPrimitive.Portal>
      <TooltipPrimitive.Content
        sideOffset={sideOffset}
        className={cn(
          'z-50 max-w-xs rounded-md bg-foreground px-2.5 py-1.5 text-xs text-background shadow-md',
          'animate-in fade-in-0 zoom-in-95',
          className,
        )}
        {...props}
      >
        {children}
      </TooltipPrimitive.Content>
    </TooltipPrimitive.Portal>
  )
}

interface TooltipProps {
  content: ReactNode
  children: ReactNode
  side?: 'top' | 'right' | 'bottom' | 'left'
  /** Keyboard shortcut hint shown next to the text. */
  shortcut?: string
}

/** Convenience wrapper: <Tooltip content={t('...')}><Button/></Tooltip>. */
export function Tooltip({ content, children, side, shortcut }: TooltipProps) {
  if (content == null || content === '') return <>{children}</>
  return (
    <TooltipRoot>
      <TooltipTrigger asChild>{children}</TooltipTrigger>
      <TooltipContent side={side}>
        <span>{content}</span>
        {shortcut ? <kbd className="ml-2 rounded bg-background/20 px-1 font-sans text-[10px] opacity-80">{shortcut}</kbd> : null}
      </TooltipContent>
    </TooltipRoot>
  )
}
