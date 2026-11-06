'use client'

import { useTranslation } from 'react-i18next'

import { useChapterContext } from '@/components/context/data'
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
  const chapter = useChapterContext().data
  const speaker = chapter?.context.lines[layer.id]?.speaker?.value
  const character = chapter?.series?.characters.find((character) => character.id === speaker)
  const label = category ? t(`lettering.categories.${category.category}`) : fallback
  const kind = character
    ? t('boxSpeaker.kindAndSpeaker', { kind: label, speaker: character.name })
    : label
  return layer.word_styles ? t('richTextPanel.styledWords', { category: kind }) : kind
}
