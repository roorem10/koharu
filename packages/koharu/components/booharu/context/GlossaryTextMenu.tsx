'use client'

import { useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { AddToGlossary } from '@/components/booharu/context/AddToGlossary'
import { newTerm, termText, type FullEntry } from '@/lib/booharu/context'
import { openExplanation } from '@/lib/booharu/explain'
import type { BlockLayer } from '@/lib/document'
import type { PageId } from '@koharu/ipc/protocol'
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@koharu/ui/components/context-menu'

/** The elements a line's text shows in, in the menu's children: Story's fields, or Box details'
 * read-only texts. */
const texts = 'textarea, dd'

/** The text selected in `element`, a field or a read-only text: a field's selection, else the
 * page's while it lies within `element`. */
function selectedIn(element: Element): string {
  if (element instanceof HTMLTextAreaElement || element instanceof HTMLInputElement) {
    return element.value.slice(element.selectionStart ?? 0, element.selectionEnd ?? 0)
  }
  const selection = window.getSelection()
  if (!selection || selection.isCollapsed) return ''
  const inside = (node: Node | null) => node !== null && element.contains(node)
  return inside(selection.anchorNode) && inside(selection.focusNode) ? selection.toString() : ''
}

/** Booharu 0.0.4's GlossaryTextMenu: a right-click on a line's source or translation, the first and
 * the second text of `children`, opens a menu with Add to glossary, which opens the term form with
 * the source selected in the source, else the line's `source`, and the translation selected in the
 * translation, else the line's `translation`; see AddToGlossary. Given the `line`, the block of the
 * page it's on, as in Story, it also has Explain this line, which opens the line's explanation in
 * Box details, for a line with a source to explain. The menu replaces the browser's own there, as
 * 0.0.4's did. */
export function GlossaryTextMenu({
  source,
  translation,
  line,
  children,
}: {
  source: string
  translation: string
  line?: { page: PageId; block: BlockLayer }
  children: ReactNode
}) {
  const { t } = useTranslation()
  // What the right-click found: which text it was on, and what was selected there.
  const clicked = useRef<{ field: number; selected: string }>({ field: -1, selected: '' })
  const [adding, setAdding] = useState<FullEntry | null>(null)

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger
          // Selecting a read-only text stays possible, as the trigger's own class stops it.
          className='grid min-w-0 gap-1 select-text'
          onContextMenuCapture={(event) => {
            const target = event.target instanceof Element ? event.target.closest(texts) : null
            const all = Array.from(event.currentTarget.querySelectorAll(texts))
            clicked.current = {
              field: target ? all.indexOf(target) : -1,
              selected: target ? termText(selectedIn(target)) : '',
            }
          }}
        >
          {children}
        </ContextMenuTrigger>
        <ContextMenuContent className='min-w-32'>
          <ContextMenuItem
            onClick={() => {
              const { field, selected } = clicked.current
              setAdding(
                newTerm(
                  field === 0 && selected ? selected : termText(source),
                  field === 1 && selected ? selected : termText(translation),
                ),
              )
            }}
          >
            {t('booharu.context.addToGlossary')}
          </ContextMenuItem>
          {line && (
            <ContextMenuItem
              disabled={!source.trim()}
              onClick={() => void openExplanation(line.page, line.block)}
            >
              {t('booharu.context.explain')}
            </ContextMenuItem>
          )}
        </ContextMenuContent>
      </ContextMenu>
      {adding && <AddToGlossary initial={adding} onClose={() => setAdding(null)} />}
    </>
  )
}
