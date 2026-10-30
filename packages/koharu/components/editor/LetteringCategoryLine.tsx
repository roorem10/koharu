'use client'

import { useQuery } from '@tanstack/react-query'
import { useTranslation } from 'react-i18next'

import { useEffectiveCategory } from '@/components/lettering/queries'
import { call } from '@/lib/backend'
import { LETTERING_CATEGORIES, useLineCategory, useSetLineCategory } from '@/lib/line-labels'
import { useProject } from '@/lib/queries'
import { commands, type EntityId, type LetteringCategory } from '@koharu/bridge/protocol'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

export function LetteringCategoryLine({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const label = useLineCategory(layer)
  const effective = useEffectiveCategory(layer)
  const setCategory = useSetLineCategory()
  const revision = useProject().data?.revision
  const shape = useQuery({
    queryKey: ['bubble-shape', layer, revision],
    queryFn: () => call(commands.getTextBubbleShape, layer),
    enabled: effective.data?.reason === 'bubble_shape',
  })
  const category = effective.data
    ? t(`lettering.categories.${effective.data.category}`)
    : t('lettering.loading')
  const reason =
    effective.data?.reason === 'bubble_shape'
      ? shape.isPending
        ? t('lettering.loading')
        : t(
            `lettering.shapeReasons.${shape.data?.bubble === null ? 'free' : (shape.data?.shape ?? 'unknown')}`,
          )
      : effective.data
        ? t(`lettering.reasons.${effective.data.reason}`)
        : ''
  const value = label.data?.source === 'user' ? label.data.value : 'automatic'
  return (
    <div className='grid min-w-0 gap-1' data-testid='lettering-category'>
      <Select
        value={value}
        disabled={
          label.isPending ||
          effective.isPending ||
          setCategory.isPending ||
          label.isError ||
          effective.isError
        }
        onValueChange={(value) => {
          if (value === null) return
          setCategory.mutate({
            layer,
            category: value === 'automatic' ? null : (value as LetteringCategory),
          })
        }}
      >
        <SelectTrigger
          size='sm'
          className='w-full min-w-0'
          aria-label={t('lettering.categoryPicker')}
        >
          <SelectValue>{category}</SelectValue>
        </SelectTrigger>
        <SelectContent alignItemWithTrigger={false}>
          <SelectItem value='automatic'>{t('lettering.automatic')}</SelectItem>
          {LETTERING_CATEGORIES.map((category) => (
            <SelectItem key={category} value={category}>
              {t(`lettering.categories.${category}`)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <span className='text-[10px] leading-4 text-muted-foreground'>
        {effective.isError ? t('lettering.loadError') : reason}
      </span>
    </div>
  )
}
