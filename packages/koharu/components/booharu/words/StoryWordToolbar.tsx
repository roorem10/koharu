'use client'

import { Bold, Italic, Star } from 'lucide-react'
import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactElement,
} from 'react'
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
import { chosenShare, wordToolbarId } from '@/lib/booharu/wordSelection'
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
 * focus. B and I show pressed as the type bar's do, by what the words' characters share.
 *
 * From the keyboard it's a toolbar: Alt+F10 in the field, while the canvas isn't typing, moves the
 * focus to its one tab stop, the control focused last; the arrows, Home and End move along it, and
 * Escape or Tab take the focus back to the field, its selection as it was, as does the toolbar
 * going while it holds the focus. A key pressed there without Ctrl, Cmd or Alt never reaches the
 * canvas's shortcuts, so Space presses the focused control and Delete removes no box, while Ctrl+Z
 * still undoes. */
export function StoryWordToolbar({ layer }: { layer: BlockLayer | TextLayer }) {
  const { t } = useTranslation()
  const range = useWordRange((state) => (state.range?.layer === layer.id ? state.range : null))
  const field = useWordRange((state) => (state.range?.layer === layer.id ? state.field : null))
  const bar = useRef<HTMLDivElement>(null)
  const [place, setPlace] = useState<Place | null>(null)
  // The control holding the toolbar's one tab stop: the one focused last.
  const [stop, setStop] = useState(0)

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

  // Words going while the toolbar holds the focus, as when an undo, a run or the agent changes
  // their text, give the focus back to the field, not to the page, where the canvas's shortcuts
  // would take Escape and Delete. The store tells of it while the toolbar is still there to ask.
  useEffect(() => {
    if (!field) return
    return useWordRange.subscribe((state) => {
      const going = state.range?.layer !== layer.id || state.field !== field
      if (going && field.isConnected && bar.current?.contains(document.activeElement)) {
        field.focus()
      }
    })
  }, [field, layer.id])

  if (!range || !field) return null
  // A text gone, as an undo takes a first translation away, lets the words go, and the toolbar
  // with them, only once the store tells of it.
  const { chars, mixed } = chosenShare(letteringOf(layer) ?? {}, range)
  const bold = isBold(chars, mixed)
  const italic = isItalic(chars, mixed)
  const style = (change: WordChange) => () => void styleWords(range, change)
  const tab = (at: number) => (at === stop ? 0 : -1)
  const keys = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return
    event.stopPropagation()
    if (event.key === 'Escape' || event.key === 'Tab') {
      event.preventDefault()
      field.focus()
      return
    }
    const controls = [...event.currentTarget.querySelectorAll('button')]
    const at = controls.indexOf(event.target as HTMLButtonElement)
    const count = controls.length
    const moves: Partial<Record<string, number>> = {
      ArrowRight: (at + 1) % count,
      ArrowLeft: (at + count - 1) % count,
      Home: 0,
      End: count - 1,
    }
    const to = moves[event.key]
    if (at < 0 || to === undefined) return
    event.preventDefault()
    controls[to].focus()
  }

  return createPortal(
    <div
      ref={bar}
      id={wordToolbarId(layer.id)}
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
      onFocus={(event) => {
        const controls: Element[] = [...event.currentTarget.querySelectorAll('button')]
        const at = controls.indexOf(event.target)
        if (at >= 0) setStop(at)
      }}
      onKeyDown={keys}
    >
      <Labelled label={t('booharu.words.bold')}>
        <Toggle
          variant='outline'
          size='sm'
          aria-label={t('booharu.words.bold')}
          className={TOGGLE}
          tabIndex={tab(0)}
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
          tabIndex={tab(1)}
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
          tabIndex={tab(2)}
          onClick={style({ emphasis: true })}
        >
          <Star aria-hidden='true' className='size-3.5' />
        </button>
      </Labelled>
      <button type='button' className={BUTTON} tabIndex={tab(3)} onClick={style({ clear: true })}>
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
