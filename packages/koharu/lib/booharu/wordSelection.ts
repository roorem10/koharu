'use client'

import {
  useEffect,
  useRef,
  type KeyboardEvent,
  type PointerEvent,
  type SyntheticEvent,
} from 'react'

import { sharedChars } from '@/components/editor/TypeBar'
import { typingShortcut } from '@/lib/booharu/typingKeys'
import {
  boldWords,
  chosenText,
  isBold,
  isItalic,
  italicWords,
  keepsWords,
  keepWords,
  setWordRange,
  styleWords,
  useWordRange,
  utf8Offset,
  type WordRange,
} from '@/lib/booharu/words'
import { letteringOf, plainText, type BlockLayer, type TextLayer } from '@/lib/document'
import type { Text } from '@koharu/ipc/protocol'

// Booharu: words chosen in the Story panel's fields (S10-l), as Booharu 0.0.4's layer rows chose
// them, which the word toolbar by the field and the type bar style. Kept apart from words.ts, which
// the type bar imports, as it reads the type bar's sharedChars.

/** The event the desktop sends the page when its canvas takes a press, which the page never sees,
 * as the window gives the canvas its presses (koharu-desktop's `booharu::canvas::pressed`). */
export const CANVAS_PRESS = 'booharu:canvas-press'

/** The id of the word toolbar of `layer`'s Story field, which Alt+F10 there moves the focus to. */
export function wordToolbarId(layer: string): string {
  return `booharu-word-toolbar-${layer}`
}

/** What the characters of `text` that the run `range` holds share, each one's marks over the
 * text's style, and the values they don't, as the type bar reads its texts'. */
export function chosenShare(text: Text, range: WordRange): ReturnType<typeof sharedChars> {
  return sharedChars([chosenText(text, range)])
}

/** Presses B, or I, for the run `range` holds of `text`, as the word toolbar's B and I do: bold,
 * or italic, unless every character already is. */
export function toggleWords(
  kind: 'bold' | 'italic',
  range: WordRange,
  text: Text,
): Promise<boolean> {
  const { chars, mixed } = chosenShare(text, range)
  return styleWords(
    range,
    kind === 'bold' ? boldWords(isBold(chars, mixed)) : italicWords(isItalic(chars, mixed)),
  )
}

/** The handlers that choose words in `layer`'s Story field, a block's translation's or a text
 * layer's, which spread on it. Selecting characters chooses them, while the field holds the saved
 * text, as offsets only match it: not in the moment a typed draft waits to be saved. A selection
 * of none, or one in a draft, lets any chosen words go, so words stay chosen only where the last
 * selection was.
 *
 * As 0.0.4's rows did, the words go too with Escape in the field, a change of the saved text,
 * another field choosing words, the field going, and a press outside the field, its word toolbar
 * and the type bar, popups they open included, a press on the canvas among them, which the desktop
 * tells the page of with `CANVAS_PRESS`; a press elsewhere in the sidebar hides the toolbar and
 * keeps the words. Ctrl+B and Ctrl+I in the field do what the toolbar's B and I do, and Alt+F10,
 * as in editors with a floating toolbar, moves the focus to the toolbar, the field keeping its
 * selection. */
export function useWordSelection(layer: BlockLayer | TextLayer) {
  const lettering = letteringOf(layer)
  const saved = plainText(lettering)
  const mine = useWordRange((state) => state.range?.layer === layer.id)
  // The field, whose sidebar a press is looked for in, once the toolbar has gone.
  const field = useRef<HTMLTextAreaElement | null>(null)

  // A change of the saved text, as saving a typed draft, an undo or a translation makes, lets the
  // words go, as they may be gone; another text's are left chosen.
  useEffect(() => {
    const { range } = useWordRange.getState()
    if (range?.layer === layer.id && range.text !== saved) setWordRange(null)
  }, [layer.id, saved])

  // As does the field going, as when Story scrolls its page away or another tab shows.
  useEffect(
    () => () => {
      if (useWordRange.getState().range?.layer === layer.id) setWordRange(null)
    },
    [layer.id],
  )

  // This capturing listener hears a press before React's handlers mark the ones that keep the
  // words, so it reads the mark a turn later. A press on the desktop's canvas reaches the page
  // only as the event the desktop sends then.
  useEffect(() => {
    if (!mine) return
    const press = (event: Event) => {
      window.setTimeout(() => {
        const { range } = useWordRange.getState()
        if (keepsWords(event) || range?.layer !== layer.id) return
        const sidebar = field.current?.closest('aside')
        const inSidebar = sidebar && event.target instanceof Node && sidebar.contains(event.target)
        setWordRange(inSidebar ? range : null)
      })
    }
    const canvas = () => {
      if (useWordRange.getState().range?.layer === layer.id) setWordRange(null)
    }
    document.addEventListener('pointerdown', press, true)
    window.addEventListener(CANVAS_PRESS, canvas)
    return () => {
      document.removeEventListener('pointerdown', press, true)
      window.removeEventListener(CANVAS_PRESS, canvas)
    }
  }, [mine, layer.id])

  return {
    onSelect: (event: SyntheticEvent<HTMLTextAreaElement>) => {
      const target = event.currentTarget
      field.current = target
      const { value, selectionStart, selectionEnd } = target
      const start = utf8Offset(value, selectionStart)
      const end = utf8Offset(value, selectionEnd)
      if (start < end && value === saved) {
        setWordRange({ layer: layer.id, start, end, text: saved }, target)
      } else if (useWordRange.getState().range) {
        setWordRange(null)
      }
    },
    onPointerDown: (event: PointerEvent<HTMLTextAreaElement>) => {
      field.current = event.currentTarget
      if (useWordRange.getState().range?.layer === layer.id) keepWords(event)
    },
    onKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => {
      const { range } = useWordRange.getState()
      if (range?.layer !== layer.id) return
      if (event.key === 'Escape') {
        event.stopPropagation()
        setWordRange(null)
        return
      }
      if (
        event.key === 'F10' &&
        event.altKey &&
        !(event.ctrlKey || event.metaKey || event.shiftKey)
      ) {
        event.preventDefault()
        document
          .getElementById(wordToolbarId(layer.id))
          ?.querySelector<HTMLElement>('[tabindex="0"]')
          ?.focus()
        return
      }
      const kind = typingShortcut(event.nativeEvent)
      if (kind && lettering && event.currentTarget.value === range.text) {
        event.preventDefault()
        void toggleWords(kind, range, lettering)
      }
    },
  }
}
