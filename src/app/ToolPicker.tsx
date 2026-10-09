import { useTranslation } from 'react-i18next'

import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from '@/components/ui/dialog'
import { useTabs } from '@/stores/tabs'
import { useUi } from '@/stores/ui'

import { ToolCards } from './ToolCards'

/** "+" dialog: pick a tool to open in a new tab. */
export function ToolPicker() {
  const { t } = useTranslation('tabs')
  const open = useUi((s) => s.toolPickerOpen)
  const setOpen = useUi((s) => s.setToolPickerOpen)
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('picker.title')}</DialogTitle>
          <DialogDescription>{t('picker.description')}</DialogDescription>
        </DialogHeader>
        <ToolCards
          compact
          onPick={(id) => {
            useTabs.getState().openTab(id)
            setOpen(false)
          }}
        />
      </DialogContent>
    </Dialog>
  )
}
