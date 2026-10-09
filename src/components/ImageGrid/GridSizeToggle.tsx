import { Grid2x2Icon, Grid3x3Icon, SquareIcon } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group'
import { Tooltip } from '@/components/ui/tooltip'
import { useSession, type ViewSize } from '@/stores/session'

const isViewSize = (v: string): v is ViewSize => v === 'small' || v === 'medium' || v === 'large'

/** Small / Medium / Large thumbnail toggle bound to the tab's session. */
export function GridSizeToggle({ tabId }: { tabId: string }) {
  const { t } = useTranslation('common')
  const viewSize = useSession(tabId, (s) => s.viewSize)
  const setViewSize = useSession(tabId, (s) => s.setViewSize)
  const items = [
    { value: 'small' as const, label: t('grid.sizeSmall'), Icon: Grid3x3Icon },
    { value: 'medium' as const, label: t('grid.sizeMedium'), Icon: Grid2x2Icon },
    { value: 'large' as const, label: t('grid.sizeLarge'), Icon: SquareIcon },
  ]
  return (
    <ToggleGroup
      type="single"
      value={viewSize}
      aria-label={t('grid.viewSize')}
      onValueChange={(v) => {
        if (isViewSize(v)) setViewSize(v)
      }}
    >
      {items.map(({ value, label, Icon }) => (
        <Tooltip key={value} content={label}>
          <ToggleGroupItem value={value} aria-label={label} className="px-1.5">
            <Icon />
          </ToggleGroupItem>
        </Tooltip>
      ))}
    </ToggleGroup>
  )
}
