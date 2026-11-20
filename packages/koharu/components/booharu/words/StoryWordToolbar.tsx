'use client'

import { Bold, Italic, Star } from 'lucide-react'
import { useLayoutEffect, useRef, useState, type ReactElement } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'

import {
  boldWords,
  isBold,
  isItalic,
  italicWords,
  keepWords,
  styleWords,
  useWordRange,
} from '@/lib/booharu/words'
import { chosenShare } from '@/lib/booharu/wordSelection'
import { letteringOf, type BlockLayer, type TextLayer } from '@/lib/document'
import type { WordChange } from '@koharu/ipc/protocol'
import { Toggle } from '@koharu/ui/components/toggle'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** The space between the toolbar and its field, in CSS pixels. */
const GAP = 6
/** The least space between the toolbar and the window's edges, in CSS pixels. */
const EDGE = 8

/** Where the toolbar sits: its left edge, and the top of its field less the gap, which it sits
 * above, or the bottom plus the gap, which it sits below. */
interface Place {
  left: number
  top: number
  above: boolean
}

/** The look of the type bar's toggles, so B and I look as they do there. */
const TOGGLE =
  'h-6 w-6 min-w-6 rounded-md bg-background px-0 text-muted-foreground data-[pressed]:border-primary data-[pressed]:bg-primary data-[pressed]:text-primary-foreground data-[pressed]:hover:bg-primary/90'
const BUTTON =
  'grid h-6 min-w-6 shrink-0 place-items-center rounded-md border border-input bg-background px-1.5 text-[11px] text-muted-foreground hover:bg-muted hover:text-foreground'

/** The word toolbar of `layer`'s Story field, as Booharu 0.0.4's layer rows had one: while words
 * are chosen there, B, I, the emphasis star and Clear style only them, through the words edit, as
 * one undo step each. It floats above the field, or below it near the window's top, in a portal,
 * so the sidebar's scrolling never clips it, and it never covers the words. A press on it leaves
 * the focus, and so the selection, in the field, which commits nothing as it would on losing the
 * focus. B and I show pressed as the type bar's do, by what the words' characters share. */
export function StoryWordToolbar({ layer }: { layer: BlockLayer | TextLayer }) {
  const { t } = useTranslation()
  const range = useWordRange((state) => (state.range?.layer === layer.id ? state.range : null))
  const field = useWordRange((state) => (state.range?.layer === layer.id ? state.field : null))
  const bar = useRef<HTMLDivElement>(null)
  const [place, setPlace] = useState<Place | null>(null)

  useLayoutEffect(() => {
    if (!field) {
      setPlace(null)
      return
    }
    const follow = () => {
      const box = field.getBoundingClientRect()
      const width = bar.current?.offsetWidth ?? 0
      const height = bar.current?.offsetHeight ?? 0
      const above = box.top - height - GAP >= EDGE
      setPlace({
        left: Math.max(EDGE, Math.min(box.left, window.innerWidth - EDGE - width)),
        top: above ? box.top - GAP : box.bottom + GAP,
        above,
      })
    }
    follow()
    window.addEventListener('resize', follow)
    window.addEventListener('scroll', follow, true)
    return () => {
      window.removeEventListener('resize', follow)
      window.removeEventListener('scroll', follow, true)
    }
  }, [field])

  const text = letteringOf(layer)
  if (!range || !field || !text) return null
  const { chars, mixed } = chosenShare(text, range)
  const bold = isBold(chars, mixed)
  const italic = isItalic(chars, mixed)
  const style = (change: WordChange) => () => void styleWords(range, change)

  return createPortal(
    <div
      ref={bar}
      role='toolbar'
      aria-label={t('booharu.words.toolbar')}
      data-testid='story-word-toolbar'
      className='fixed z-50 flex items-center gap-1 rounded-xl border border-border bg-[var(--surface-floating)] p-1 shadow-[var(--shadow-toolrail)]'
      style={{
        left: place?.left ?? 0,
        top: place?.top ?? 0,
        transform: place?.above ? 'translateY(-100%)' : undefined,
        visibility: place ? 'visible' : 'hidden',
      }}
      // A press here keeps the words, and the focus in the field.
      onPointerDown={(event) => {
        keepWords(event)
        event.preventDefault()
      }}
      onMouseDown={(event) => event.preventDefault()}
    >
      <Labelled label={t('booharu.words.bold')}>
        <Toggle
          variant='outline'
          size='sm'
          aria-label={t('booharu.words.bold')}
          className={TOGGLE}
          pressed={bold}
          onPressedChange={style(boldWords(bold))}
        >
          <Bold className='size-3.5' />
        </Toggle>
      </Labelled>
      <Labelled label={t('booharu.words.italic')}>
        <Toggle
          variant='outline'
          size='sm'
          aria-label={t('booharu.words.italic')}
          className={TOGGLE}
          pressed={italic}
          onPressedChange={style(italicWords(italic))}
        >
          <Italic className='size-3.5' />
        </Toggle>
      </Labelled>
      <Labelled label={t('booharu.words.emphasis')}>
        <button
          type='button'
          aria-label={t('booharu.words.emphasis')}
          className={BUTTON}
          onClick={style({ emphasis: true })}
        >
          <Star aria-hidden='true' className='size-3.5' />
        </button>
      </Labelled>
      <button type='button' className={BUTTON} onClick={style({ clear: true })}>
        {t('booharu.words.clear')}
      </button>
    </div>,
    document.body,
  )
}

/** `children`, a button, with `label` in a tooltip, as the type bar's buttons have theirs. */
function Labelled({ label, children }: { label: string; children: ReactElement }) {
  return (
    <Tooltip>
      <TooltipTrigger render={children} />
      <TooltipContent>{label}</TooltipContent>
    </Tooltip>
  )
}
