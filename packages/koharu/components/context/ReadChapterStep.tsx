'use client'

import { useKoharuStore } from '@/lib/store'

import { ReadChapterButton } from './ReadChapterButton'

export function ReadChapterStep() {
  const scope = useKoharuStore((state) => state.processingScope)
  const selected = useKoharuStore((state) => state.selectedPages)
  return (
    <ReadChapterButton
      beforeTranslation
      scope={
        scope === 'selected-pages' ? { scope: 'pages', value: selected } : { scope: 'project' }
      }
    />
  )
}
