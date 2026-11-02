'use client'

import { useQuery } from '@tanstack/react-query'
import { useEffect, useLayoutEffect, useRef, useState, type SyntheticEvent } from 'react'
import { createPortal } from 'react-dom'

import { call } from '@/lib/backend'
import { pageKey, projectKey, refresh, usePage, useProject } from '@/lib/queries'
import { utf16ToCharOffset } from '@/lib/richText'
import { useRichTextSelection } from '@/lib/richTextSelection'
import { commands, type Layer, type RunStylePatch } from '@koharu/bridge/protocol'

import { MiniToolbar } from './MiniToolbar'

type TextLayer = Extract<Layer, { type: 'text' }>

/**
 * Word styling from the layer row's Translation box: highlighting words there
 * shares them with the text panel and shows the same toolbar as the page editor.
 */
export function useRowWordToolbar(layer: TextLayer) {
  const project = useProject().data
  const page = usePage().data
  const saved = layer.content.translation?.text ?? ''
  const selection = useRichTextSelection((state) => state.selection)
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const area = useRef<HTMLTextAreaElement | null>(null)
  const toolbar = useRef<HTMLDivElement | null>(null)
  const handled = useRef(new WeakSet<Event>())
  const [position, setPosition] = useState<{ left: number; top: number; above: boolean } | null>(
    null,
  )
  const mine =
    open && selection?.layer === layer.id && selection.start < selection.end ? selection : null
  const runs = useQuery({
    queryKey: ['word-styles', project?.name, page?.id, project?.revision, layer.id],
    queryFn: () => call(commands.getWordStyles, layer.id),
    enabled: mine !== null,
  })

  const close = (keepSelection = false) => {
    setOpen(false)
    if (!keepSelection && useRichTextSelection.getState().selection?.layer === layer.id)
      useRichTextSelection.getState().setSelection(null)
  }
  const latestClose = useRef(close)
  latestClose.current = close

  const read = (event: SyntheticEvent<HTMLTextAreaElement>) => {
    const target = event.currentTarget
    area.current = target
    const a = utf16ToCharOffset(target.value, target.selectionStart)
    const b = utf16ToCharOffset(target.value, target.selectionEnd)
    // Offsets only match the saved text; unsaved typing must not be styled by position.
    if (a < b && target.value === saved) {
      useRichTextSelection.getState().setSelection({ layer: layer.id, start: a, end: b })
      setOpen(true)
    } else if (open) close()
  }

  useEffect(() => {
    if (!open) return
    const outside = (event: PointerEvent) => {
      // React handlers (including portals inside the toolbar) mark the event first.
      setTimeout(() => {
        if (handled.current.has(event)) return
        const inSidebar = event.target instanceof Element && event.target.closest('aside')
        // The text panel in the sidebar keeps acting on the words.
        latestClose.current(Boolean(inSidebar))
      })
    }
    document.addEventListener('pointerdown', outside, true)
    return () => document.removeEventListener('pointerdown', outside, true)
  }, [open])

  // Another box or the page editor took the selection.
  useEffect(() => {
    if (open && selection?.layer !== layer.id) setOpen(false)
  }, [open, selection, layer.id])

  // The text changed under the selection (typing, undo, retranslation).
  useEffect(() => {
    if (open && area.current && area.current.value !== saved) close()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [saved])

  useEffect(
    () => () => {
      if (useRichTextSelection.getState().selection?.layer === layer.id)
        useRichTextSelection.getState().setSelection(null)
    },
    [layer.id],
  )

  useLayoutEffect(() => {
    if (!mine || !area.current) {
      setPosition(null)
      return
    }
    const place = () => {
      const box = area.current!.getBoundingClientRect()
      const height = toolbar.current?.offsetHeight ?? 72
      const above = box.top - height - 8 >= 0
      setPosition({ left: Math.max(8, box.left), top: above ? box.top - 6 : box.bottom + 6, above })
    }
    place()
    window.addEventListener('resize', place)
    window.addEventListener('scroll', place, true)
    return () => {
      window.removeEventListener('resize', place)
      window.removeEventListener('scroll', place, true)
    }
  }, [mine])

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true)
    try {
      await action()
      await refresh(projectKey, pageKey, ['word-styles'], ['box-effects'])
    } catch {
      // The command layer reports failures; the toolbar stays usable.
    } finally {
      setBusy(false)
    }
  }

  const typography = layer.typography ?? null
  const baseSize = typography && !typography.auto_fit ? typography.size : null
  const toolbarNode =
    mine && position
      ? createPortal(
          <div
            data-canvas-control
            data-testid={`row-word-toolbar-${layer.id}`}
            ref={toolbar}
            className='fixed z-50 max-w-[calc(100vw-1rem)]'
            style={{
              left: position.left,
              top: position.top,
              transform: position.above ? 'translateY(-100%)' : undefined,
            }}
            onPointerDown={(event) => handled.current.add(event.nativeEvent)}
          >
            <MiniToolbar
              key={JSON.stringify([mine.start, mine.end, runs.data])}
              runs={runs.data ?? []}
              start={mine.start}
              end={mine.end}
              typography={typography}
              baseSize={baseSize}
              busy={busy || runs.isLoading}
              apply={(patch: RunStylePatch) =>
                void run(() => call(commands.setWordStyle, layer.id, mine.start, mine.end, patch))
              }
              emphasis={() =>
                void run(() => call(commands.applyEmphasis, layer.id, mine.start, mine.end))
              }
              clear={() =>
                void run(() => call(commands.clearWordStyles, [layer.id], [mine.start, mine.end]))
              }
              done={() => close()}
            />
          </div>,
          document.body,
        )
      : null

  return {
    textareaProps: {
      onSelect: read,
      onPointerDown: (event: React.PointerEvent<HTMLTextAreaElement>) =>
        handled.current.add(event.nativeEvent),
      onKeyDown: (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
        if (event.key === 'Escape' && open) {
          event.stopPropagation()
          close()
        }
      },
    },
    toolbar: toolbarNode,
  }
}
