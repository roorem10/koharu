'use client'

import { useEffect, useRef, useState, type ComponentType } from 'react'
import { useTranslation } from 'react-i18next'

import { GlossaryTextMenu } from '@/components/booharu/context/GlossaryTextMenu'
import { LineContextCard } from '@/components/booharu/context/LineContextCard'
import { LetteringSection } from '@/components/booharu/lettering/LetteringSection'
import { QcSection } from '@/components/booharu/qc/QcSection'
import { call } from '@/lib/backend'
import { reviewNote } from '@/lib/booharu/attrs'
import { boxActions, runBoxAction, type Box } from '@/lib/booharu/boxActions'
import { showInStory } from '@/lib/booharu/boxDetails'
import { useLettering } from '@/lib/booharu/boxLettering'
import { explainSection } from '@/lib/booharu/explain'
import { qcSection } from '@/lib/booharu/qc'
import { isBlockLayer, plainText } from '@/lib/document'
import { commands, type PageId } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Textarea } from '@koharu/ui/components/textarea'

/** What a Box details section shows: a text, and its page. */
export interface BoxDetailsProps {
  page: PageId
  box: Box
}

/** One part of Box details: its heading, an interface string's key, what it shows of the text, and
 * for which texts, every one when left out. */
export interface BoxDetailsSection {
  id: string
  title: string
  Section: ComponentType<BoxDetailsProps>
  shows?: (box: Box) => boolean
}

/** Box details' sections, in the order it shows them. A later one goes in as one more entry. */
export const boxDetailsSections: readonly BoxDetailsSection[] = [
  { id: 'source', title: 'booharu.details.sections.source', Section: SourceSection },
  // What the quality check found, which the ? tag opens on while something is open.
  { id: qcSection, title: 'booharu.details.sections.qc', Section: QcSection },
  {
    id: explainSection,
    title: 'booharu.details.sections.explain',
    Section: LineContextCard,
    // A text layer is no line of the page's to explain, nor is a block with no source.
    shows: ({ layer }) => isBlockLayer(layer) && plainText(layer.source).trim() !== '',
  },
  {
    id: 'lettering',
    title: 'booharu.details.sections.lettering',
    Section: LetteringSection,
    // A text layer has no category: the T tool's text keeps its own font.
    shows: ({ layer }) => isBlockLayer(layer),
  },
  { id: 'note', title: 'booharu.details.sections.note', Section: NoteSection },
  { id: 'actions', title: 'booharu.details.sections.actions', Section: ActionsSection },
]

/** The text's source and translation, or a text layer's text, short and read-only: Story edits
 * them, and Show in Story goes there. A block's, as in Story, take a right-click Add to glossary. */
function SourceSection({ page, box: { layer } }: BoxDetailsProps) {
  const { t } = useTranslation()
  const fields = isBlockLayer(layer)
    ? [
        { label: t('booharu.details.source'), text: plainText(layer.source) },
        { label: t('booharu.details.translation'), text: plainText(layer.translation) },
      ]
    : [{ label: t('booharu.details.text'), text: plainText(layer) }]
  const list = (
    <dl className='grid gap-1.5'>
      {fields.map(({ label, text }) => (
        <div key={label} className='grid gap-0.5'>
          <dt className='text-[10px] text-muted-foreground'>{label}</dt>
          <dd className='line-clamp-3 text-[12px] leading-4 break-words whitespace-pre-wrap'>
            {text || <span className='text-muted-foreground'>{t('booharu.details.empty')}</span>}
          </dd>
        </div>
      ))}
    </dl>
  )
  return (
    <div className='grid gap-2'>
      {isBlockLayer(layer) ? (
        <GlossaryTextMenu
          source={plainText(layer.source)}
          translation={plainText(layer.translation)}
        >
          {list}
        </GlossaryTextMenu>
      ) : (
        list
      )}
      <Button
        type='button'
        variant='outline'
        size='xs'
        className='justify-self-start text-[11px]'
        onClick={() => showInStory(page, layer.id)}
      >
        {t('booharu.details.showInStory')}
      </Button>
    </div>
  )
}

/** The reviewer's note on the text, saved when the field loses focus, or when Box details closes
 * while it has focus, as one undo step, and removed when it's left blank. A locked text, or one in a
 * locked layer, takes no note, as the scene refuses one, so the section says why in place of the
 * field. */
function NoteSection({ box: { layer, locked } }: BoxDetailsProps) {
  const { t } = useTranslation()
  const note = reviewNote(layer)
  // What's typed, kept until the note it saved, or another writer's, comes back.
  const [draft, setDraft] = useState<string | null>(null)
  const editing = useRef(false)
  useEffect(() => {
    if (!editing.current) setDraft(null)
  }, [note])
  // Escape closes Box details before the field can lose focus, as React hears no blur while it
  // takes the field away: what's typed then is saved as it goes, as losing focus saves it.
  const saveOnLeave = useRef<() => void>(() => undefined)
  useEffect(() => () => saveOnLeave.current(), [])

  if (locked) {
    saveOnLeave.current = () => undefined
    return (
      <div className='grid gap-1.5'>
        {note && <p className='text-[12px] leading-4 break-words whitespace-pre-wrap'>{note}</p>}
        <p className='text-[11px] leading-4 text-muted-foreground'>
          {t('booharu.details.noteLocked')}
        </p>
      </div>
    )
  }

  const save = () => {
    editing.current = false
    if (draft === null) return
    const next = draft.trim() ? draft : null
    if (next === note) {
      setDraft(null)
      return
    }
    // A failure shows its error, as every command's does, and leaves what was typed.
    void call(commands.booharuSetReviewNote, layer.id, next).catch(() => undefined)
  }
  saveOnLeave.current = () => {
    if (editing.current) save()
  }
  return (
    <Textarea
      aria-label={t('booharu.details.noteLabel')}
      placeholder={t('booharu.details.notePlaceholder')}
      value={draft ?? note ?? ''}
      rows={3}
      className='min-h-16 resize-y text-[12px]'
      onFocus={() => {
        editing.current = true
      }}
      onChange={(event) => setDraft(event.currentTarget.value)}
      onBlur={save}
    />
  )
}

/** The text's box actions, as its menus list them, those that open Box details left out: Box
 * details itself, and Change category and Explain this line, whose sections are here; and FX…,
 * whose Text effects open in the type bar, which this dialog covers. They follow the page's
 * lettering, as the menus do. */
function ActionsSection({ page, box }: BoxDetailsProps) {
  const { t } = useTranslation()
  const lettering = useLettering(page).data
  const selection = { page, boxes: [box], lettering }
  return (
    <div className='flex flex-wrap gap-1.5'>
      {boxActions
        .filter(({ id }) => !['fx', 'details', 'category', 'explain'].includes(id))
        .map((action) => (
          <Button
            key={action.id}
            type='button'
            variant='outline'
            size='xs'
            className='text-[11px]'
            disabled={!action.enabled(selection)}
            onClick={() => runBoxAction(action, selection)}
          >
            {t(action.label)}
          </Button>
        ))}
    </div>
  )
}
