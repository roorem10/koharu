'use client'

import { Lightbulb } from 'lucide-react'
import { useTranslation } from 'react-i18next'

import type { BooharuTurn } from '@/lib/booharu/agentChat'
import { useKoharuStore } from '@/lib/store'
import { modelName } from '@/lib/translation'

type Divider = Extract<BooharuTurn, { role: 'divider' }>
type Note = Extract<BooharuTurn, { role: 'note' }>

/** One of Booharu's rows in the agent's chat (S7-k): where a new conversation starts, the model
 * remembering nothing above it, or Explain this line's answer, which the model never heard. */
export function ChatRow({ turn }: { turn: BooharuTurn }) {
  return turn.role === 'divider' ? <DividerRow divider={turn} /> : <NoteRow note={turn} />
}

/** Previous conversation, as Booharu 0.0.4 marked where one ended, over a rule across the chat;
 * with why when the model's memory couldn't be read back, rather than the user's New chat. */
function DividerRow({ divider }: { divider: Divider }) {
  const { t } = useTranslation()
  return (
    <div className='grid gap-1'>
      <p className='flex items-center gap-2 text-[10px] text-muted-foreground'>
        <span aria-hidden className='h-px flex-1 bg-border' />
        {t('booharu.agent.divider')}
        <span aria-hidden className='h-px flex-1 bg-border' />
      </p>
      {divider.reason === 'memory_lost' && (
        <p className='mx-auto max-w-64 text-center text-[10px] leading-4 text-muted-foreground'>
          {t('booharu.agent.memoryLost')}
        </p>
      )}
    </div>
  )
}

/** Explain this line's answer, as the line card showed it: the line as it was asked about, the
 * explanation, the other translations, and which model answered. */
function NoteRow({ note }: { note: Note }) {
  const { t } = useTranslation()
  const models = useKoharuStore((state) => state.models)
  return (
    <article
      aria-label={t('booharu.agent.explained')}
      className='grid min-w-0 gap-1.5 rounded-xl border border-border/70 bg-muted/30 px-3 py-2 text-[11px] leading-4'
    >
      <p className='flex items-center gap-1.5 text-[10px] font-semibold text-muted-foreground'>
        <Lightbulb aria-hidden className='size-3 shrink-0' />
        {t('booharu.agent.explained')}
      </p>
      <div className='grid gap-0.5 border-l-2 border-border pl-2 select-text'>
        <p className='break-words whitespace-pre-wrap'>{note.source}</p>
        {note.translation && (
          <p className='break-words whitespace-pre-wrap text-muted-foreground'>
            {note.translation}
          </p>
        )}
      </div>
      <p className='break-words whitespace-pre-wrap select-text'>{note.explanation}</p>
      {note.alternatives.length > 0 && (
        <ul
          aria-label={t('booharu.context.alternatives')}
          className='grid list-disc gap-0.5 pl-4 select-text'
        >
          {note.alternatives.map((text, index) => (
            <li key={index} className='break-words whitespace-pre-wrap'>
              {text}
            </li>
          ))}
        </ul>
      )}
      <p className='text-[10px] text-muted-foreground'>
        {t('booharu.context.answeredBy', { model: modelName(models, note.model) })}
      </p>
    </article>
  )
}
