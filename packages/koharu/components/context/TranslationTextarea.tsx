'use client'

import { useRef, type ComponentProps } from 'react'

import { CommitTextarea } from '@/components/controls/CommitTextarea'
import { call } from '@/lib/backend'
import { pageKey, projectKey, refresh, useProject } from '@/lib/queries'
import { commands, type Layer } from '@koharu/bridge/protocol'

import { offerTranslationTerm, TermLearningOffer } from './TermLearningOffer'

type TextLayer = Extract<Layer, { type: 'text' }>

/** Learning waits for a completed edit session, including its last asynchronous autosave. */
export function TranslationTextarea({
  layer,
  ...props
}: Omit<ComponentProps<typeof CommitTextarea>, 'value' | 'onCommit'> & { layer: TextLayer }) {
  const project = useProject().data?.name
  const initial = useRef({ text: layer.content.translation?.text ?? '', project })
  const pending = useRef<Promise<void>>(Promise.resolve())
  return (
    <div
      className='min-w-0'
      onFocusCapture={(event) => {
        if (event.target instanceof HTMLTextAreaElement)
          initial.current = { text: layer.content.translation?.text ?? '', project }
      }}
      onBlur={(event) => {
        if (!(event.target instanceof HTMLTextAreaElement)) return
        const before = initial.current,
          after = event.target.value
        void pending.current
          .then(() =>
            offerTranslationTerm(
              layer.id,
              layer.content.source?.text ?? '',
              before.text,
              after,
              before.project,
            ),
          )
          .catch(() => undefined)
      }}
    >
      <CommitTextarea
        {...props}
        value={layer.content.translation?.text ?? ''}
        onCommit={(text) => {
          pending.current = call(commands.setTranslation, layer.id, text.trim() ? text : null).then(
            () => refresh(projectKey, pageKey),
          )
          void pending.current.catch(() => undefined)
        }}
      />
      <TermLearningOffer layer={layer.id} />
    </div>
  )
}
