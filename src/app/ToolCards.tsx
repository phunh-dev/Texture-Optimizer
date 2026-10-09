import { ArrowRightIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { cn } from '@/lib/utils'
import { tools, type ToolId } from '@/tabs/registry'

const ACCENTS: Record<ToolId, string> = {
  resize: 'from-sky-500/15 to-sky-500/5 text-sky-600 dark:text-sky-400',
  resolution: 'from-violet-500/15 to-violet-500/5 text-violet-600 dark:text-violet-400',
  trim: 'from-emerald-500/15 to-emerald-500/5 text-emerald-600 dark:text-emerald-400',
  potPad: 'from-amber-500/15 to-amber-500/5 text-amber-600 dark:text-amber-400',
  atlas: 'from-indigo-500/15 to-indigo-500/5 text-indigo-600 dark:text-indigo-400',
  rename: 'from-pink-500/15 to-pink-500/5 text-pink-600 dark:text-pink-400',
  bgRemove: 'from-rose-500/15 to-rose-500/5 text-rose-600 dark:text-rose-400',
  meshPack: 'from-teal-500/15 to-teal-500/5 text-teal-600 dark:text-teal-400',
}

/** Grid of tool cards (icon, name, description); used by the home screen and the tool picker. */
export function ToolCards({ onPick, compact }: { onPick: (id: ToolId) => void; compact?: boolean }) {
  const { t } = useTranslation('tabs')
  return (
    <div className={cn('grid gap-3', compact ? 'grid-cols-2' : 'grid-cols-[repeat(auto-fill,minmax(240px,1fr))]')}>
      {tools.map((tool) => {
        const Icon = tool.icon
        return (
          <button
            key={tool.id}
            type="button"
            data-testid={`tool-card-${tool.id}`}
            onClick={() => onPick(tool.id)}
            className="group/card relative flex cursor-default flex-col items-start gap-3 rounded-xl border border-border bg-card p-4 text-left shadow-xs outline-none transition-all duration-200 hover:-translate-y-0.5 hover:border-primary/40 hover:shadow-lg focus-visible:ring-2 focus-visible:ring-ring/60"
          >
            <div className="flex w-full items-center justify-between">
              <span className={cn('flex size-10 items-center justify-center rounded-xl bg-gradient-to-br', ACCENTS[tool.id])}>
                <Icon className="size-5" />
              </span>
              <ArrowRightIcon className="size-4 -translate-x-1 text-primary opacity-0 transition-all group-hover/card:translate-x-0 group-hover/card:opacity-100" />
            </div>
            <div className="space-y-1">
              <h3 className="text-sm font-semibold">{t(tool.titleKey)}</h3>
              <p className={cn('text-xs leading-relaxed text-muted-foreground', compact && 'line-clamp-2')}>{t(tool.descriptionKey)}</p>
            </div>
          </button>
        )
      })}
    </div>
  )
}
