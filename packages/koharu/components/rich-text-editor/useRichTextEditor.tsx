'use client'

import { useEffect, useLayoutEffect, useReducer, useRef, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { offerTranslationTerm } from '@/components/context/TermLearningOffer'
import { originalVisible, useCompareStore } from '@/components/qc/compare'
import { call } from '@/lib/backend'
import { effectiveLayerVisibility } from '@/lib/document'
import { controlFrame, pagePoint } from '@/lib/geometry'
import { pageKey, pagesKey, projectKey, refresh, usePage, useProject } from '@/lib/queries'
import { charLength, charToUtf16Offset, utf16ToCharOffset } from '@/lib/richText'
import { useRichTextSelection } from '@/lib/richTextSelection'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type EntityId,
  type GlyphBox,
  type Layer,
  type Point,
  type RunStylePatch,
  type StyleRun,
} from '@koharu/bridge/protocol'

import { MiniToolbar } from './MiniToolbar'
import {
  adjacentLine,
  caretEdge,
  frameQuad,
  lineBoundary,
  nearestOffset,
  selectedStyles,
  selectionQuads,
  stepOffset,
  stylePatch,
  textChange,
  type XY,
} from './model'

type TextLayer = Extract<Layer, { type: 'text' }>
type Session = {
  layer: TextLayer
  page: EntityId
  project: string | undefined
  saved: string
  initial: string
  draft: string
  glyphText: string
  anchor: number
  focus: number
  glyphs: GlyphBox[]
  runs: StyleRun[]
  loading: boolean
  composing: boolean
  closing: boolean
  pending: number
  request: number
  timer: ReturnType<typeof setTimeout> | null
  error: boolean
  /** Groups this session's typing commits into one undo step. */
  group: number
}
let editingSessions = 0
const nextEditingSession = () => (editingSessions = (editingSessions + 1) % 2 ** 31)
/** Typing is saved this soon after a keystroke so the page shows it almost live. */
const TYPING_SAVE_MS = 60
const shownText = (layer: TextLayer) =>
  layer.content.translation?.text ?? layer.content.source?.text ?? ''

export function useRichTextEditor(): {
  open: (layer: EntityId, point: Point, event: MouseEvent) => boolean
  editing: EntityId | null
  overlay: ReactNode
} {
  const { t } = useTranslation()
  const page = usePage().data
  const project = useProject().data?.name
  const camera = useKoharuStore((state) => state.camera)
  const frames = useKoharuStore((state) => state.layerFrames)
  const tool = useKoharuStore((state) => state.tool)
  const selected = useKoharuStore((state) => state.selectedLayers)
  const revision = useKoharuStore((state) => state.canvasRevision)
  const generation = useKoharuStore((state) => state.canvasGeneration)
  const showingOriginal = useCompareStore((state) => state.pinned || state.held)
  const session = useRef<Session | null>(null)
  const input = useRef<HTMLTextAreaElement>(null)
  const root = useRef<HTMLDivElement>(null)
  const toolbar = useRef<HTMLDivElement>(null)
  const handled = useRef(new WeakSet<Event>())
  const drag = useRef<{
    pointer: number
    anchor: number
    point: Point
    ready: boolean
    released: boolean
  } | null>(null)
  const queue = useRef<Promise<unknown>>(Promise.resolve())
  const mounted = useRef(true)
  const [, redraw] = useReducer((n: number) => n + 1, 0)
  const update = () => {
    if (mounted.current) redraw()
  }
  const current = useRef({ page, project })
  current.current = { page, project }

  const enqueue = <T,>(operation: () => Promise<T>): Promise<T> => {
    const task = queue.current.then(operation)
    queue.current = task.catch(() => undefined)
    return task
  }
  const active = (s: Session) => session.current === s && mounted.current
  const range = (s: Session): [number, number] => [
    Math.min(s.anchor, s.focus),
    Math.max(s.anchor, s.focus),
  ]
  const publish = (s: Session) => {
    if (!active(s)) return
    const [start, end] = range(s)
    // A panel must never apply the draft's offsets to text that has not been saved yet.
    useRichTextSelection
      .getState()
      .setSelection(
        start < end && s.saved === s.draft && !s.pending ? { layer: s.layer.id, start, end } : null,
      )
    update()
  }
  const readGeometry = async (s: Session) => {
    const request = ++s.request
    const text = s.saved
    const [glyphs, runs] = await Promise.all([
      call(commands.getTextGlyphBoxes, s.layer.id),
      call(commands.getWordStyles, s.layer.id),
    ])
    if (!active(s) || request !== s.request || text !== s.saved) return
    s.glyphs = glyphs.filter(
      (g) =>
        g.end > g.start &&
        g.end <= charLength(text) &&
        g.quad.every((p) => p.every(Number.isFinite)),
    )
    s.glyphText = text
    s.runs = runs
    s.loading = false
    update()
  }
  const flush = (s: Session): Promise<void> => {
    if (s.timer !== null) clearTimeout(s.timer)
    s.timer = null
    if (s.composing) return Promise.resolve()
    const text = s.draft
    s.pending++
    return enqueue(async () => {
      if (text === s.saved) return
      if (s.project !== current.current.project) throw new Error(t('richText.projectChanged'))
      const change = textChange(s.saved, text)
      await call(
        commands.replaceTextRange,
        s.layer.id,
        change.start,
        change.end,
        change.insert,
        s.group,
      )
      s.saved = text
      s.error = false
      await refresh(projectKey, pagesKey, pageKey)
      await readGeometry(s)
    })
      .catch((error) => {
        s.error = true
        throw error
      })
      .finally(() => {
        s.pending--
        publish(s)
      })
  }
  const focusInput = () => input.current?.focus({ preventScroll: true })
  const leave = async (s: Session) => {
    s.closing = true
    update()
    if (s.composing && input.current) {
      s.draft = input.current.value
      s.composing = false
      input.current.blur()
    }
    try {
      do {
        await flush(s)
      } while (s.draft !== s.saved)
      if (!active(s)) return
      session.current = null
      drag.current = null
      useRichTextSelection.getState().setSelection(null)
      void offerTranslationTerm(
        s.layer.id,
        s.layer.content.source?.text ?? '',
        s.initial,
        s.saved,
        s.project,
      ).catch(() => undefined)
      update()
    } catch {
      // Retain the draft when a native edit failed so the user can retry it.
      s.closing = false
      update()
    }
  }
  const select = (s: Session, anchor: number, focus: number) => {
    const length = charLength(s.draft)
    s.anchor = Math.max(0, Math.min(length, anchor))
    s.focus = Math.max(0, Math.min(length, focus))
    publish(s)
  }
  const changeText = (s: Session, text: string, anchor: number, focus: number) => {
    if (!active(s)) return
    s.draft = text
    s.error = false
    select(s, anchor, focus)
    if (s.timer !== null) clearTimeout(s.timer)
    if (!s.composing)
      s.timer = setTimeout(() => {
        void flush(s).catch(() => update())
      }, TYPING_SAVE_MS)
  }
  const insert = (s: Session, text: string, start = range(s)[0], end = range(s)[1]) => {
    const draft =
      s.draft.slice(0, charToUtf16Offset(s.draft, start)) +
      text +
      s.draft.slice(charToUtf16Offset(s.draft, end))
    changeText(s, draft, start + charLength(text), start + charLength(text))
  }
  const style = (s: Session, operation: (start: number, end: number) => Promise<unknown>) => {
    const [start, end] = range(s)
    if (start === end || s.composing) return
    void flush(s)
      .then(() =>
        enqueue(async () => {
          if (!active(s)) return
          s.loading = true
          update()
          try {
            await operation(start, end)
            await refresh(projectKey, pagesKey, pageKey)
            await readGeometry(s)
          } finally {
            s.loading = false
            publish(s)
          }
        }),
      )
      .catch(() => update())
  }
  const toggle = (s: Session, kind: 'bold' | 'italic') => {
    const [start, end] = range(s)
    const values = selectedStyles(s.runs, start, end, s.layer.typography)
    const patch =
      kind === 'bold'
        ? values.weight !== null && values.weight >= 600
          ? stylePatch({}, ['font_weight'])
          : stylePatch({ font_weight: 700 })
        : values.italic === 'italic'
          ? stylePatch({}, ['font_style'])
          : stylePatch({ font_style: 'italic' })
    style(s, (start, end) => call(commands.setWordStyle, s.layer.id, start, end, patch))
  }

  const open = (id: EntityId, point: Point, event: MouseEvent): boolean => {
    const layer = current.current.page?.layers.find((layer) => layer.id === id)
    const state = useKoharuStore.getState()
    if (
      layer?.type !== 'text' ||
      !current.current.page ||
      state.tool !== 'select' ||
      originalVisible() ||
      event.button !== 0 ||
      (event.target instanceof Element && event.target.closest('[data-canvas-control]'))
    )
      return false
    const visibility = effectiveLayerVisibility(current.current.page.layers, layer)
    if (!visibility.visible || visibility.opacity <= 0) return false
    const previous = session.current
    if (previous) {
      void leave(previous).then(() => {
        if (!session.current) open(id, point, event)
      })
      return true
    }
    const text = shownText(layer)
    const s: Session = {
      layer,
      page: current.current.page.id,
      project: current.current.project,
      saved: text,
      initial: layer.content.translation?.text ?? '',
      draft: text,
      glyphText: text,
      anchor: 0,
      focus: 0,
      glyphs: [],
      runs: [],
      loading: true,
      composing: false,
      closing: false,
      pending: 0,
      request: 0,
      timer: null,
      error: false,
      group: nextEditingSession(),
    }
    session.current = s
    state.selectLayers([id])
    useRichTextSelection.getState().setSelection(null)
    update()
    void readGeometry(s)
      .then(() => {
        if (!active(s)) return
        // Entering the editor only places the caret; words are picked by dragging.
        const offset = nearestOffset(
          s.glyphs,
          point,
          s.layer.typography?.writing_mode === 'Vertical',
        )
        select(s, offset, offset)
        focusInput()
      })
      .catch(() => {
        if (active(s)) {
          s.loading = false
          s.error = true
          update()
          focusInput()
        }
      })
    return true
  }

  const latest = useRef({ leave, flush, readGeometry })
  latest.current = { leave, flush, readGeometry }
  useEffect(() => {
    mounted.current = true
    const outside = (event: PointerEvent) => {
      // Portal events still bubble through React's overlay. Let them mark the event
      // first: a microtask would run between native listeners, before React sees the
      // event, and close the editor under a toolbar click.
      setTimeout(() => {
        const s = session.current
        if (!s || handled.current.has(event)) return
        if (event.target instanceof Element && event.target.closest('aside')) {
          void latest.current.flush(s).catch(() => update())
          return
        }
        void latest.current.leave(s)
      })
    }
    document.addEventListener('pointerdown', outside, true)
    return () => {
      document.removeEventListener('pointerdown', outside, true)
      const s = session.current
      if (s) {
        s.composing = false
        void latest.current.flush(s).catch(() => undefined)
      }
      mounted.current = false
      session.current = null
      useRichTextSelection.getState().setSelection(null)
    }
  }, [])

  useEffect(() => {
    const s = session.current
    if (!s) return
    if (
      s.page !== page?.id ||
      s.project !== project ||
      tool !== 'select' ||
      showingOriginal ||
      selected.length !== 1 ||
      selected[0] !== s.layer.id
    ) {
      void latest.current.leave(s)
      return
    }
    const layer = page?.layers.find((layer) => layer.id === s.layer.id)
    if (layer?.type !== 'text') {
      void latest.current.leave(s)
      return
    }
    s.layer = layer
    // A stale page query must not overwrite an in-flight typing chunk.
    if (!s.pending && s.draft === s.saved && shownText(layer) !== s.saved) {
      s.saved = shownText(layer)
      s.draft = s.saved
      s.anchor = Math.min(s.anchor, charLength(s.draft))
      s.focus = Math.min(s.focus, charLength(s.draft))
      publish(s)
    }
    if (!s.pending && !s.loading) void latest.current.readGeometry(s).catch(() => update())
  }, [page, project, tool, selected, showingOriginal, revision, generation])

  const s = session.current
  useLayoutEffect(() => {
    const area = input.current
    if (!s || !area || s.composing) return
    const [start, end] = range(s)
    area.setSelectionRange(
      charToUtf16Offset(s.draft, start),
      charToUtf16Offset(s.draft, end),
      s.focus < s.anchor ? 'backward' : 'forward',
    )
  })
  useLayoutEffect(() => {
    const bar = toolbar.current,
      host = root.current
    if (!bar || !host) return
    const bounds = bar.getBoundingClientRect(),
      viewport = host.getBoundingClientRect()
    const left = Number.parseFloat(bar.style.left)
    if (bounds.right > viewport.right - 4)
      bar.style.left = `${Math.max(4, left - (bounds.right - viewport.right) - 4)}px`
  })

  if (!s) return { open, editing: null, overlay: null }
  const vertical = s.layer.typography?.writing_mode === 'Vertical'
  const frame = controlFrame(s.layer, frames)
  const framePoints = frame ? frameQuad(frame) : null
  const dpr = window.devicePixelRatio || 1
  const screen = ([x, y]: XY): XY => [
    (x * camera.zoom + camera.translation[0]) / dpr,
    (y * camera.zoom + camera.translation[1]) / dpr,
  ]
  const local = (event: { clientX: number; clientY: number }) =>
    pagePoint(
      event.clientX,
      event.clientY,
      root.current!.getBoundingClientRect(),
      useKoharuStore.getState().camera,
    )
  const points = (quad: XY[]) => quad.map((point) => screen(point).join(',')).join(' ')
  const [start, end] = range(s)
  const validGeometry = s.glyphText === s.draft
  const quads = validGeometry ? selectionQuads(s.glyphs, start, end, vertical) : []
  const caret = validGeometry
    ? (caretEdge(s.glyphs, s.focus, vertical) ??
      (framePoints ? ([framePoints[0], framePoints[vertical ? 1 : 3]] as [XY, XY]) : null))
    : null
  const boundary = (quads.length ? quads.flat() : (framePoints ?? ([[0, 0]] as XY[]))).map(screen)
  const left = Math.max(4, Math.min(...boundary.map(([x]) => x)))
  const top = Math.min(...boundary.map(([, y]) => y))
  const bottom = Math.max(...boundary.map(([, y]) => y))
  const caretScreen = caret ? screen(caret[0]) : [left, Math.max(0, top)]
  const baseSize =
    s.layer.typography && !s.layer.typography.auto_fit ? s.layer.typography.size : null
  const keyboard = (event: React.KeyboardEvent<HTMLTextAreaElement>) => {
    event.stopPropagation()
    if (event.nativeEvent.isComposing || s.composing || event.keyCode === 229) return
    const command = event.ctrlKey || event.metaKey,
      key = event.key.toLowerCase()
    if (event.key === 'Escape') {
      event.preventDefault()
      void leave(s)
      return
    }
    if (command && (key === 'z' || key === 'y')) {
      event.preventDefault()
      const undo = key === 'y' || event.shiftKey ? commands.redo : commands.undo
      void flush(s)
        .then(() =>
          enqueue(async () => {
            await call(undo)
            await refresh(projectKey, pagesKey, pageKey)
          }),
        )
        .catch(() => update())
      return
    }
    if (command && key === 'a') {
      event.preventDefault()
      select(s, 0, charLength(s.draft))
      return
    }
    if (command && (key === 'b' || key === 'i')) {
      event.preventDefault()
      toggle(s, key === 'b' ? 'bold' : 'italic')
      return
    }
    if (event.key === 'Backspace' || event.key === 'Delete') {
      event.preventDefault()
      if (start !== end) insert(s, '')
      else if (event.key === 'Backspace')
        insert(s, '', stepOffset(s.draft, s.focus, -1, command), s.focus)
      else insert(s, '', s.focus, stepOffset(s.draft, s.focus, 1, command))
      return
    }
    let next: number | null = null
    const backward = vertical ? 'ArrowUp' : 'ArrowLeft',
      forward = vertical ? 'ArrowDown' : 'ArrowRight'
    if (event.key === backward || event.key === forward) {
      const delta = event.key === backward ? -1 : 1
      next =
        !event.shiftKey && start !== end
          ? delta < 0
            ? start
            : end
          : stepOffset(s.draft, s.focus, delta, command)
    } else if (event.key === 'Home' || event.key === 'End') {
      next = command
        ? event.key === 'End'
          ? charLength(s.draft)
          : 0
        : lineBoundary(s.glyphs, s.focus, event.key === 'End', charLength(s.draft))
    } else if (event.key.startsWith('Arrow')) {
      const delta = event.key === (vertical ? 'ArrowLeft' : 'ArrowDown') ? 1 : -1
      next = adjacentLine(s.glyphs, s.focus, delta, vertical)
    }
    if (next !== null) {
      event.preventDefault()
      select(s, event.shiftKey ? s.anchor : next, next)
    }
  }

  const overlay = (
    <div
      ref={root}
      data-canvas-control
      data-rich-text-overlay
      className='pointer-events-none absolute inset-0 z-30'
      onPointerDown={(event) => {
        handled.current.add(event.nativeEvent)
        event.stopPropagation()
      }}
      onPointerMove={(event) => event.stopPropagation()}
      onPointerUp={(event) => event.stopPropagation()}
      onDoubleClick={(event) => event.stopPropagation()}
      onContextMenu={(event) => event.stopPropagation()}
      onWheel={(event) => {
        if (!(event.target instanceof Element && event.target.closest('[data-rich-text-glyphs]')))
          event.stopPropagation()
      }}
      onKeyDown={(event) => {
        event.stopPropagation()
        if (event.key === 'Escape' && !event.nativeEvent.isComposing) {
          event.preventDefault()
          void leave(s)
        }
      }}
    >
      <svg
        data-canvas-control
        data-rich-text-glyphs
        aria-hidden
        className='absolute inset-0 size-full overflow-visible'
      >
        {(framePoints || s.glyphs.length > 0) && (
          <g
            data-canvas-control
            className='pointer-events-auto cursor-text'
            onPointerDown={(event) => {
              if (event.button !== 0 || s.loading || s.closing || s.composing) return
              event.preventDefault()
              event.currentTarget.setPointerCapture(event.pointerId)
              const point = local(event),
                pointer = event.pointerId,
                extend = event.shiftKey
              const gesture = { pointer, anchor: s.anchor, point, ready: false, released: false }
              drag.current = gesture
              const place = () => {
                if (!active(s) || drag.current !== gesture) return
                const offset = nearestOffset(s.glyphs, point, vertical)
                const anchor = extend ? s.anchor : offset
                gesture.anchor = anchor
                gesture.ready = true
                select(s, anchor, nearestOffset(s.glyphs, gesture.point, vertical))
                if (gesture.released) drag.current = null
                focusInput()
              }
              if (!validGeometry)
                void flush(s)
                  .then(place)
                  .catch(() => update())
              else place()
            }}
            onPointerMove={(event) => {
              const gesture = drag.current
              if (gesture?.pointer !== event.pointerId) return
              event.preventDefault()
              gesture.point = local(event)
              if (!gesture.ready) return
              select(s, gesture.anchor, nearestOffset(s.glyphs, local(event), vertical))
            }}
            onPointerUp={(event) => {
              if (drag.current?.pointer !== event.pointerId) return
              drag.current.point = local(event)
              drag.current.released = true
              if (!drag.current.ready) return
              select(s, drag.current.anchor, nearestOffset(s.glyphs, local(event), vertical))
              drag.current = null
              event.currentTarget.releasePointerCapture(event.pointerId)
              if (s.draft !== s.saved) void flush(s).catch(() => update())
            }}
            onPointerCancel={() => {
              drag.current = null
            }}
            onDoubleClick={(event) => {
              // Words are picked by dragging, so a double-click keeps the caret where it is.
              event.preventDefault()
              focusInput()
            }}
          >
            <polygon
              data-canvas-control
              data-testid='rich-text-frame'
              points={points(
                framePoints ?? [
                  s.glyphs[0].quad[0],
                  s.glyphs.at(-1)!.quad[1],
                  s.glyphs.at(-1)!.quad[2],
                  s.glyphs[0].quad[3],
                ],
              )}
              fill='transparent'
              stroke='var(--canvas-selection)'
              strokeWidth={1.5}
              strokeDasharray='5 3'
            />
            {s.glyphs.map((glyph, i) => (
              <polygon data-canvas-control key={i} points={points(glyph.quad)} fill='transparent' />
            ))}
          </g>
        )}
        {quads.map((quad, i) => (
          <polygon
            data-canvas-control
            key={i}
            points={points(quad)}
            fill='var(--canvas-selection)'
            fillOpacity={0.3}
          />
        ))}
        {start === end && caret && (
          <line
            data-canvas-control
            data-testid='rich-text-caret'
            x1={screen(caret[0])[0]}
            y1={screen(caret[0])[1]}
            x2={screen(caret[1])[0]}
            y2={screen(caret[1])[1]}
            stroke='var(--canvas-selection)'
            strokeWidth={1.5}
          />
        )}
      </svg>
      <textarea
        data-canvas-control
        ref={input}
        aria-label={t('richText.title')}
        className='pointer-events-auto absolute size-px resize-none overflow-hidden border-0 p-0 opacity-0'
        style={{ left: caretScreen[0], top: caretScreen[1] }}
        value={s.draft}
        spellCheck={false}
        autoCapitalize='off'
        autoComplete='off'
        readOnly={s.closing}
        onKeyDown={keyboard}
        onChange={(event) => {
          const area = event.currentTarget
          const a = utf16ToCharOffset(area.value, area.selectionStart),
            b = utf16ToCharOffset(area.value, area.selectionEnd)
          changeText(
            s,
            area.value,
            area.selectionDirection === 'backward' ? b : a,
            area.selectionDirection === 'backward' ? a : b,
          )
        }}
        onCompositionStart={() => {
          s.composing = true
          if (s.timer !== null) clearTimeout(s.timer)
          s.timer = null
          update()
        }}
        onCompositionEnd={(event) => {
          s.composing = false
          const area = event.currentTarget,
            offset = utf16ToCharOffset(area.value, area.selectionEnd)
          changeText(s, area.value, offset, offset)
        }}
        onPaste={(event) => {
          event.preventDefault()
          insert(s, event.clipboardData.getData('text/plain'))
        }}
        onCopy={(event) => {
          event.preventDefault()
          event.clipboardData.setData(
            'text/plain',
            s.draft.slice(charToUtf16Offset(s.draft, start), charToUtf16Offset(s.draft, end)),
          )
        }}
        onCut={(event) => {
          event.preventDefault()
          event.clipboardData.setData(
            'text/plain',
            s.draft.slice(charToUtf16Offset(s.draft, start), charToUtf16Offset(s.draft, end)),
          )
          insert(s, '')
        }}
      />
      {start < end && !s.composing && (
        <div
          data-canvas-control
          ref={toolbar}
          className='pointer-events-auto absolute max-w-full'
          style={{
            left,
            ...(top >= 80 ? { top: top - 8, transform: 'translateY(-100%)' } : { top: bottom + 8 }),
          }}
        >
          <MiniToolbar
            key={JSON.stringify([start, end, s.runs])}
            runs={s.runs}
            start={start}
            end={end}
            typography={s.layer.typography}
            baseSize={baseSize}
            busy={s.loading || s.pending > 0}
            apply={(patch: RunStylePatch) =>
              style(s, (start, end) => call(commands.setWordStyle, s.layer.id, start, end, patch))
            }
            emphasis={() =>
              style(s, (start, end) => call(commands.applyEmphasis, s.layer.id, start, end))
            }
            clear={() =>
              style(s, (start, end) => call(commands.clearWordStyles, [s.layer.id], [start, end]))
            }
            done={() => {
              void leave(s)
            }}
          />
        </div>
      )}
      {(s.error || s.loading) && (
        <div
          data-canvas-control
          role='status'
          className='pointer-events-auto absolute max-h-40 max-w-sm overflow-auto rounded-md border border-border bg-popover p-2 text-xs text-popover-foreground shadow'
          style={{ left, top: bottom + (start < end && top < 80 ? 80 : 8) }}
        >
          {s.error ? t('richText.saveFailed') : t('richText.loading')}
          {s.error && (
            <button
              data-canvas-control
              type='button'
              className='ml-2 underline'
              onClick={() => {
                void leave(s)
              }}
            >
              {t('richText.retry')}
            </button>
          )}
        </div>
      )}
    </div>
  )
  return { open, editing: s.layer.id, overlay }
}
