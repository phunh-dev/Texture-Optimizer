import { ImageIcon, XIcon } from 'lucide-react'
import { memo, useState, type MouseEvent } from 'react'
import { useTranslation } from 'react-i18next'

import { Badge } from '@/components/ui/misc'
import { Tooltip } from '@/components/ui/tooltip'
import { inTauri } from '@/lib/env'
import { thumbnailUrl } from '@/lib/ipc'
import type { ImportedFile } from '@/lib/ipc/types'
import { cn, dimensionWarnings } from '@/lib/utils'
import type { ViewSize } from '@/stores/session'

import { GRID_SIZES } from './sizes'

interface ImageCellProps {
  file: ImportedFile
  domId: string
  size: ViewSize
  selected: boolean
  focused: boolean
  onSelect: (file: ImportedFile, e: MouseEvent) => void
  onRemove: (file: ImportedFile) => void
}

function Thumbnail({ file, size }: { file: ImportedFile; size: ViewSize }) {
  const [failed, setFailed] = useState(false)
  const src = inTauri() && !failed ? thumbnailUrl(file, size) : null
  if (!src) {
    return (
      <div data-testid="thumb-placeholder" className="flex size-full items-center justify-center bg-muted text-muted-foreground/60">
        <ImageIcon className={size === 'small' ? 'size-6' : 'size-10'} strokeWidth={1.25} />
      </div>
    )
  }
  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      decoding="async"
      draggable={false}
      onError={() => setFailed(true)}
      className="size-full object-contain"
    />
  )
}

export const ImageCell = memo(function ImageCell({ file, domId, size, selected, focused, onSelect, onRemove }: ImageCellProps) {
  const { t } = useTranslation('common')
  const metrics = GRID_SIZES[size]
  const warnings = dimensionWarnings(file.width, file.height)
  const small = size === 'small'

  return (
    <div
      id={domId}
      role="gridcell"
      aria-selected={selected}
      aria-label={file.name}
      data-size={size}
      data-file-id={file.id}
      onClick={(e) => onSelect(file, e)}
      style={{ width: metrics.cell, height: metrics.cell + metrics.caption }}
      className={cn(
        'group/cell relative flex flex-col overflow-hidden rounded-lg border bg-card transition-[box-shadow,border-color,transform] duration-150',
        selected ? 'border-primary ring-2 ring-primary/40' : 'border-border hover:border-foreground/25 hover:shadow-md',
        focused && 'outline-2 outline-offset-2 outline-ring',
      )}
    >
      <div className="bg-checker relative overflow-hidden" style={{ width: metrics.cell - 2, height: metrics.cell - 2 }}>
        <Thumbnail file={file} size={size} />

        {(warnings.nonPot || warnings.notMultipleOf4) && (
          <div className="pointer-events-auto absolute bottom-1 left-1 flex gap-1">
            {warnings.nonPot && (
              <Tooltip content={t('grid.nonPot')}>
                <Badge variant="warning" data-testid="badge-npot" aria-label={t('grid.nonPot')}>
                  {t('grid.nonPotShort')}
                </Badge>
              </Tooltip>
            )}
            {warnings.notMultipleOf4 && (
              <Tooltip content={t('grid.notMultipleOf4')}>
                <Badge variant="destructive" data-testid="badge-mul4" aria-label={t('grid.notMultipleOf4')}>
                  {t('grid.notMultipleOf4Short')}
                </Badge>
              </Tooltip>
            )}
          </div>
        )}

        {selected && <div className="pointer-events-none absolute inset-0 bg-primary/10" />}
      </div>

      <div className={cn('flex min-w-0 flex-col justify-center px-2', small ? 'py-0.5' : 'gap-0.5 py-1')} style={{ height: metrics.caption }}>
        <Tooltip content={file.name}>
          <span className={cn('truncate font-medium text-foreground', small ? 'text-[10px]' : 'text-xs')}>{file.name}</span>
        </Tooltip>
        {small ? null : (
          <span className="truncate text-[11px] tabular-nums text-muted-foreground">
            {t('grid.dimensions', { width: file.width, height: file.height })}
          </span>
        )}
      </div>

      <button
        type="button"
        aria-label={t('grid.removeImage', { name: file.name })}
        data-testid="remove-badge"
        onClick={(e) => {
          e.stopPropagation()
          onRemove(file)
        }}
        className={cn(
          'absolute right-1 top-1 flex items-center justify-center rounded-full bg-foreground/75 text-background shadow-sm backdrop-blur-sm transition-[opacity,transform,background-color] outline-none',
          'hover:scale-110 hover:bg-destructive focus-visible:opacity-100 focus-visible:ring-2 focus-visible:ring-ring',
          small ? 'size-4 opacity-100' : 'size-6 opacity-0 group-hover/cell:opacity-100 group-focus-within/cell:opacity-100',
        )}
      >
        <XIcon className={small ? 'size-2.5' : 'size-3.5'} strokeWidth={2.5} />
      </button>
    </div>
  )
})
