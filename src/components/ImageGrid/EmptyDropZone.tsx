import { FolderPlusIcon, ImagePlusIcon, UploadCloudIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'

interface EmptyDropZoneProps {
  dragOver: boolean
  onAddFiles: () => void
  onAddFolder: () => void
}

export function EmptyDropZone({ dragOver, onAddFiles, onAddFolder }: EmptyDropZoneProps) {
  const { t } = useTranslation('common')
  return (
    <div className="flex size-full items-center justify-center p-6" data-testid="empty-drop-zone">
      <div
        className={cn(
          'flex w-full max-w-2xl flex-col items-center gap-5 rounded-2xl border-2 border-dashed px-8 py-14 text-center transition-all duration-200',
          dragOver ? 'scale-[1.01] border-primary bg-primary/5' : 'border-border bg-card/40',
        )}
      >
        <div
          className={cn(
            'flex size-16 items-center justify-center rounded-2xl transition-colors',
            dragOver ? 'bg-primary text-primary-foreground' : 'bg-accent text-primary',
          )}
        >
          <UploadCloudIcon className="size-8" strokeWidth={1.5} />
        </div>
        <div className="space-y-1.5">
          <h2 className="text-lg font-semibold">{dragOver ? t('import.dropActive') : t('import.dropTitle')}</h2>
          <p className="mx-auto max-w-md text-sm text-muted-foreground">{t('import.dropHint')}</p>
        </div>
        <div className="flex flex-wrap items-center justify-center gap-2">
          <Button onClick={onAddFiles}>
            <ImagePlusIcon />
            {t('actions.addFiles')}
          </Button>
          <span className="text-xs text-muted-foreground">{t('import.or')}</span>
          <Button variant="outline" onClick={onAddFolder}>
            <FolderPlusIcon />
            {t('actions.addFolder')}
          </Button>
        </div>
      </div>
    </div>
  )
}
