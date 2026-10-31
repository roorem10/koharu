'use client'

import { useTranslation } from 'react-i18next'

import { useEffectiveCategory } from '@/components/lettering/queries'
import type { Layer } from '@koharu/bridge/protocol'

export function TextLayerKind({
  layer,
  fallback,
}: {
  layer: Extract<Layer, { type: 'text' }>
  fallback: string
}) {
  const { t } = useTranslation()
  const category = useEffectiveCategory(layer.id).data
  const label = category ? t(`lettering.categories.${category.category}`) : fallback
  return layer.word_styles ? t('richTextPanel.styledWords', { category: label }) : label
}
