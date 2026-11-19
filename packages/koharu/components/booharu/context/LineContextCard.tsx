'use client'

import { LoaderCircle } from 'lucide-react'
import { useId } from 'react'
import { Trans, useTranslation } from 'react-i18next'

import type { BoxDetailsProps } from '@/components/booharu/details/sections'
import { discussInAgent } from '@/lib/booharu/agentChat'
import { readNote } from '@/lib/booharu/attrs'
import { closeBoxDetails } from '@/lib/booharu/boxDetails'
import { answerFor, explainLine, useExplainModel, useExplanation } from '@/lib/booharu/explain'
import { readingSection } from '@/lib/booharu/reading'
import { openSettingsSection } from '@/lib/booharu/settingsLink'
import { commitTranslation } from '@/lib/booharu/translation'
import { isBlockLayer } from '@/lib/document'
import { useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { modelName } from '@/lib/translation'
import { Button } from '@koharu/ui/components/button'

/** Booharu 0.0.4's LineContextCard, as Box details' Explain this line section for a block: Read
 * chapter's note on the line, then Explain this line, which asks the model Explain this line uses
 * about the line, as it is, in its page and chapter. Its answer explains the line and gives two or
 * three other ways to translate it, each with Use this, which makes it the line's translation as
 * typing it in Story does, unless the line is locked; it says which model answered, and Discuss
 * takes the line to the agent's chat. An answer goes once the line changes since it was asked, as
 * by Use this. A failure says why, with Try again. With no model to ask, Explain this line is off,
 * and the card says where to choose one. */
export function LineContextCard({ box: { layer, locked } }: BoxDetailsProps) {
  const { t, i18n } = useTranslation()
  const alternatives = useId()
  const project = useProject().data?.name ?? null
  const explanation = useExplanation(project, layer.id)
  const { model, known } = useExplainModel()
  const models = useKoharuStore((state) => state.models)
  if (!isBlockLayer(layer)) return null

  const note = readNote(layer)
  const answer = answerFor(explanation, layer)
  const explaining = explanation?.state === 'explaining'
  const failed = explanation?.state === 'failed' ? explanation.error : null

  return (
    <div className='grid gap-2 text-[12px] leading-4'>
      {note && (
        <div className='grid gap-0.5'>
          <p className='text-[10px] text-muted-foreground'>{t('booharu.context.readNote')}</p>
          <p className='break-words whitespace-pre-wrap select-text'>{note}</p>
        </div>
      )}
      {failed && (
        <p role='alert' className='break-words whitespace-pre-wrap text-destructive select-text'>
          {failed}
        </p>
      )}
      {/* A polite live region that stays, so a screen reader tells the explaining and the answer
          as they come: the focused Explain this line goes as the answer shows. */}
      <div role='status' aria-live='polite' className='grid gap-2'>
        {!answer && (
          <Button
            type='button'
            variant='outline'
            size='xs'
            className='justify-self-start text-[11px]'
            disabled={explaining || !known || model === null}
            onClick={() => void explainLine(layer.id)}
          >
            {explaining && <LoaderCircle aria-hidden className='animate-spin' />}
            {t(
              explaining
                ? 'booharu.context.explaining'
                : failed
                  ? 'errors.tryAgain'
                  : 'booharu.context.explain',
            )}
          </Button>
        )}
        {!answer && model === null && (
          <p className='text-[11px] leading-4 text-muted-foreground'>
            <Trans
              t={t}
              i18n={i18n}
              i18nKey='booharu.context.explainNeedsModel'
              values={{
                setting: t('booharu.context.readingModel'),
                tab: t('settings.tabs.pipeline'),
              }}
              components={{
                a: (
                  <button
                    type='button'
                    className='text-primary underline underline-offset-2 hover:text-foreground'
                    onClick={() => openSettingsSection('pipeline', readingSection)}
                  />
                ),
              }}
            />
          </p>
        )}
        {answer && (
          <div className='grid gap-2 rounded-md border border-border/70 bg-muted/30 p-2'>
            <p className='break-words whitespace-pre-wrap select-text'>{answer.explanation}</p>
            {answer.alternatives.length > 0 && (
              <ul aria-label={t('booharu.context.alternatives')} className='grid gap-1.5'>
                {answer.alternatives.map((text, index) => (
                  <li
                    key={index}
                    className='flex min-w-0 items-start gap-2 border-t border-border/60 pt-1.5'
                  >
                    <p
                      id={`${alternatives}-${index}`}
                      className='min-w-0 flex-1 break-words whitespace-pre-wrap select-text'
                    >
                      {text}
                    </p>
                    <Button
                      type='button'
                      variant='outline'
                      size='xs'
                      className='shrink-0 text-[11px]'
                      aria-describedby={`${alternatives}-${index}`}
                      disabled={locked}
                      onClick={() => commitTranslation(layer, text)}
                    >
                      {t('booharu.context.useThis')}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
            {locked && answer.alternatives.length > 0 && (
              <p className='text-[11px] leading-4 text-muted-foreground'>
                {t('booharu.context.useThisLocked')}
              </p>
            )}
            <div className='flex min-w-0 items-center gap-2'>
              <p
                className='min-w-0 flex-1 truncate text-[10px] text-muted-foreground'
                title={modelName(models, answer.model)}
              >
                {t('booharu.context.answeredBy', { model: modelName(models, answer.model) })}
              </p>
              <Button
                type='button'
                variant='ghost'
                size='xs'
                className='shrink-0 text-[11px]'
                onClick={() => {
                  closeBoxDetails()
                  discussInAgent(
                    t('booharu.context.discussPrompt', {
                      layer: layer.id,
                      source: answer.source,
                      translation: answer.translation,
                      explanation: answer.explanation,
                    }),
                  )
                }}
              >
                {t('booharu.context.discuss')}
              </Button>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
