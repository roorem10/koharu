'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import { create } from 'zustand'

import { call } from '@/lib/backend'
import { pageKey, projectKey, queryClient, refresh, usePage, useProject } from '@/lib/queries'
import { commands, type EntityId, type Page, type ProjectInfo } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'

import { refreshContext } from './data'
import { detectLearnedTerm, type LearnedTerm } from './term-learning'
import { retranslateTerm } from './term-usage'

type Offer = LearnedTerm & {
  project: string
  layer: EntityId
  series: string
  sourceText: string
  translation: string
}

// Only the displayed offer is transient. Native context remembers which choices were offered.
const useTermOffer = create<{ offer: Offer | null }>()(() => ({ offer: null }))

function currentTranslation(layer: EntityId, source: string, translation: string): boolean {
  const page = queryClient.getQueryData<Page | null>(pageKey)
  const text = page?.layers.find((item) => item.id === layer)
  return (
    text?.type === 'text' &&
    text.content.source?.text === source &&
    text.content.translation?.text === translation
  )
}

/** Called only after a successful hand edit, never in response to a query or pipeline update. */
export async function offerTranslationTerm(
  layer: EntityId,
  source: string,
  before: string,
  after: string,
  project: string | undefined,
): Promise<void> {
  if (before === after || !before || !currentTranslation(layer, source, after)) return
  if (!project || queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name !== project) return
  const current = () =>
    queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name === project &&
    currentTranslation(layer, source, after)
  const chapter = await call(commands.getChapterContext)
  if (!chapter.series || !current()) return
  const known = [
    ...(chapter.series.glossary ?? []).map((term) => ({
      source: term.source ?? '',
      target: term.target ?? '',
    })),
    ...(chapter.series.characters ?? []).map((character) => ({
      source: character.original ?? '',
      target: character.name ?? '',
    })),
  ]
  const learned = detectLearnedTerm(source, before, after, known)
  if (!learned || !current()) return
  const series = chapter.series.id!
  const claimed = await call(
    commands.claimTermOffer,
    series,
    learned.source,
    learned.before,
    learned.after,
    project,
  )
  if (claimed && current()) {
    useTermOffer.setState({
      offer: { ...learned, project, layer, series, sourceText: source, translation: after },
    })
  }
}

export function TermLearningOffer({ layer }: { layer: EntityId }) {
  const { t } = useTranslation()
  const offer = useTermOffer((state) => state.offer)
  const project = useProject().data
  const page = usePage().data
  const [busy, setBusy] = useState(false)
  const text = page?.layers.find((item) => item.id === layer)
  if (
    !offer ||
    offer.layer !== layer ||
    offer.project !== project?.name ||
    text?.type !== 'text' ||
    text.content.source?.text !== offer.sourceText ||
    text.content.translation?.text !== offer.translation
  )
    return null
  const dismiss = () => {
    if (useTermOffer.getState().offer === offer) useTermOffer.setState({ offer: null })
  }
  const useEverywhere = async () => {
    const current = () =>
      queryClient.getQueryData<ProjectInfo | null>(projectKey)?.name === offer.project &&
      currentTranslation(layer, offer.sourceText, offer.translation)
    if (busy || !current()) return
    setBusy(true)
    try {
      await call(
        commands.updateSeriesTerm,
        offer.series,
        offer.source,
        offer.before,
        offer.after,
        offer.project,
      )
      await Promise.all([refreshContext(), refresh(projectKey, pageKey)])
      if (current()) {
        const usage = await call(commands.glossaryUsage, offer.source)
        if (current()) await retranslateTerm(usage)
      }
      dismiss()
    } finally {
      setBusy(false)
    }
  }
  return (
    <div className='grid min-w-0 gap-1 rounded-md border bg-muted/30 p-2 text-xs' role='status'>
      <p className='break-words'>{t('chapterContext.learnedTerm', offer)}</p>
      <Button
        size='sm'
        variant='outline'
        className='h-auto min-h-8 w-full text-left whitespace-normal'
        disabled={busy}
        onClick={() => void useEverywhere().catch(() => undefined)}
      >
        {t('chapterContext.useEverywhereAndSave')}
      </Button>
      <Button size='sm' variant='ghost' disabled={busy} onClick={dismiss}>
        {t('chapterContext.dismiss')}
      </Button>
    </div>
  )
}
