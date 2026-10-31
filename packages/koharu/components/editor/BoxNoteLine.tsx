'use client'

import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { queryClient, useProject } from '@/lib/queries'
import type { EntityId } from '@koharu/bridge/protocol'
import { commands } from '@koharu/bridge/protocol'
import { Textarea } from '@koharu/ui/components/textarea'

export function BoxNoteLine({ layer }: { layer: EntityId }) {
  const project = useProject().data?.name
  return <NoteEditor key={JSON.stringify([project, layer])} layer={layer} project={project} />
}

function NoteEditor({ layer, project }: { layer: EntityId; project: string | undefined }) {
  const { t } = useTranslation()
  const queryKey = ['box-note', project, layer]
  const note = useQuery({
    queryKey,
    queryFn: () => call(commands.getBoxNote, layer),
    staleTime: 0,
    networkMode: 'always',
  })
  const [open, setOpen] = useState(false)
  const [draft, setDraft] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [failed, setFailed] = useState(false)
  const text = draft ?? note.data ?? ''

  useEffect(() => {
    if (note.data) setOpen(true)
  }, [note.data])

  const save = () => {
    if (saving || note.isPending || note.isError) return
    const next = text.trim() ? text : null
    if (next === note.data) {
      setDraft(null)
      return
    }
    setSaving(true)
    setFailed(false)
    void call(commands.setBoxNote, layer, next)
      .then(() => {
        queryClient.setQueryData(queryKey, next)
        setDraft(null)
      })
      .catch(() => setFailed(true))
      .finally(() => setSaving(false))
  }
  return (
    <div className='grid min-w-0 gap-1 border-t pt-1 text-[10px]'>
      <button
        type='button'
        className='min-w-0 text-left text-muted-foreground hover:text-foreground'
        aria-expanded={open}
        onClick={() => setOpen((value) => !value)}
      >
        {t('textExtras.note')}
      </button>
      {open && (
        <Textarea
          aria-label={t('textExtras.boxNote')}
          value={text}
          disabled={note.isPending || note.isError || saving}
          rows={2}
          className='min-h-12 min-w-0 resize-y text-[11px]'
          placeholder={t('textExtras.notePlaceholder')}
          onChange={(event) => {
            setDraft(event.currentTarget.value)
            setFailed(false)
          }}
          onBlur={save}
        />
      )}
      {(note.isError || failed) && (
        <span role='alert'>
          {t(note.isError ? 'textExtras.noteLoadError' : 'textExtras.noteSaveError')}
        </span>
      )}
      {saving && <span role='status'>{t('textExtras.saving')}</span>}
    </div>
  )
}
