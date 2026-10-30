'use client'

import { useTranslation } from 'react-i18next'

import { useLineCategory } from '@/lib/line-labels'
import type { EntityId } from '@koharu/bridge/protocol'

// Owned by WO-19, which turns this into the category picker.
export function LetteringCategoryLine({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const label = useLineCategory(layer).data
  return (
    <span className='block text-[10px] text-muted-foreground' data-testid='lettering-category'>
      {t('lettering.categoryLine', {
        category: label ? t(`lettering.categories.${label.value}`) : t('lettering.automatic'),
      })}
    </span>
  )
}
