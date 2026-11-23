'use client'

import { useMutation } from '@tanstack/react-query'
import { AlignCenter, AlignLeft, AlignRight, ChevronDown, Italic } from 'lucide-react'
import { useEffect, useId, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { FxPopover } from '@/components/booharu/effects/FxPopover' // Booharu: S12-k.
import { AppliesTo, WholeBoxOnly } from '@/components/booharu/typebar/AppliesTo' // Booharu: S11-f.
import { BorderExtras } from '@/components/booharu/typebar/BorderExtras' // Booharu: 2nd border (Q2).
import { ClearWordStyles } from '@/components/booharu/typebar/ClearWordStyles' // Booharu: S11-g.
import { DoneButton } from '@/components/booharu/typebar/DoneButton' // Booharu: S11-c.
import { Spacing } from '@/components/booharu/typebar/Spacing' // Booharu: S11-i, S12-k.
import { WordSizeSteps } from '@/components/booharu/typebar/WordSizeSteps' // Booharu: S11-e.
import { BoldToggle } from '@/components/booharu/words/BoldToggle' // Booharu: B (D12).
import { EmphasisButton } from '@/components/booharu/words/EmphasisButton' // Booharu: the star.
import { Choice } from '@/components/controls/Choice'
import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { ContextBar } from '@/components/editor/ContextBar'
import { useBooharuFontPicker } from '@/lib/booharu/fonts' // Booharu: favourites and local names.
import { useFxBar } from '@/lib/booharu/fx' // Booharu: Text effects goes with the bar (S12-k).
// Booharu: the values the selected texts don't share (S11-h).
import { borderCompanion, paragraphShare, setEach, type BorderKey } from '@/lib/booharu/mixed'
import { charsOr, sharedPaint } from '@/lib/booharu/paint' // Booharu: a fill and a gradient, one paint.
import {
  chosenText,
  isItalic,
  italicWords,
  keepWords,
  styleWords,
  unmarkTyping,
  useChosenWords,
  weightChoices,
} from '@/lib/booharu/words' // Booharu: I, the weights (D12) and words chosen in Story.
import { toHex, toRgba } from '@/lib/color'
import {
  expandLayerSelection,
  isBlockLayer,
  isTextLayer,
  letteringOf,
  walkLayers,
} from '@/lib/document'
import { readPage, send, useFonts, usePage, usePreferences } from '@/lib/queries'
import { setStyle, updateLayer } from '@/lib/scene'
import { useKoharuStore, type CanvasTool } from '@/lib/store'
import {
  commands,
  type Align,
  type CharKey,
  type CharStyle,
  type Direction,
  type FontFamily,
  type Layer, // Booharu: a style edit of each target's own (S11-h).
  type LayerId,
  type Text,
  type TextStyle,
  type Value,
  type WordChange, // Booharu: words chosen in Story.
} from '@koharu/ipc/protocol'
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from '@koharu/ui/components/dropdown-menu'
import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'
import { Toggle } from '@koharu/ui/components/toggle'
import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'
import { cn } from '@koharu/ui/lib/utils'

/** The renderer's own default when `[renderer]` names no font. */
const FALLBACK_FONT = 'CCWildWords'
const BLACK = '#000000'
const WHITE = '#FFFFFF'

/** The tools whose selection the bar follows; a brush's settings are on the tool rail. */
const TOOLS: CanvasTool[] = ['select', 'direct_select', 'text']

/** The selected texts' type settings in a bar beside them on the canvas, as Photoshop's Contextual
 * Task Bar shows a selection's type settings on the canvas: centred below the texts, or above when
 * there's no room below, kept inside the canvas, on one row or two when the canvas is narrower.
 * Every setting a text's lettering has: the style it draws with, its direction, and its box's
 * fit. As Photoshop's Character panel, while the canvas types, a character setting styles the
 * selected characters, or the next ones typed at a caret; otherwise every character of every text
 * and block selected. A value the characters don't share shows empty. */
export function TypeBar() {
  const { t } = useTranslation()
  const tool = useKoharuStore((state) => state.tool)
  const page = usePage().data
  const selectedIds = useKoharuStore((state) => state.selectedLayers)
  const defaultFont = usePreferences().data?.renderer.font_families?.[0] ?? FALLBACK_FONT
  const availableFonts = useFonts().data
  const booharuFonts = useBooharuFontPicker() // Booharu: favourites and local names.
  const targets = useMemo(() => {
    if (!page) return []
    const leaves = expandLayerSelection(page.layers, selectedIds)
    return walkLayers(page.layers)
      .map(({ layer }) => layer)
      .filter((layer) => leaves.includes(layer.id) && (isTextLayer(layer) || isBlockLayer(layer)))
  }, [page, selectedIds])
  const current = targets[0]
  // Booharu: the first lettered target's, as a block with no translation yet has none and the
  // values the targets share leave it out (S11-h).
  const lettering: Text | null = targets.map(letteringOf).find((text) => text !== null) ?? null
  const typing = useKoharuStore((state) => state.typing)
  const chosen = useChosenWords(targets, typing) // Booharu: words chosen in Story in the one text shown.
  // The newest style asked for shows on the first target until the page is read with it.
  const restyle = useMutation({
    mutationFn: (edit: {
      layer: LayerId
      style: TextStyle
      targets: Layer[]
      patch: TextStyle
      each?: (layer: Layer) => TextStyle | null // Booharu: each target's own patch (S11-h).
    }) => (edit.each ? setEach(edit.targets, edit.each) : setStyle(edit.targets, edit.patch)),
    onSettled: readPage,
  })

  // While typing, what the canvas says its selected characters share; otherwise what every
  // character of the selected texts shares.
  const shared = useMemo(
    () =>
      typing ??
      sharedChars(
        // Booharu: or with words chosen in Story, what their characters share.
        chosen && lettering
          ? [chosenText(lettering, chosen)]
          : targets.flatMap((layer) => letteringOf(layer) ?? []),
      ),
    [typing, targets, chosen, lettering],
  )
  const drafted =
    current && restyle.isPending && restyle.variables.layer === current.id
      ? restyle.variables.style
      : null
  const style: TextStyle = {
    ...lettering?.style,
    ...drafted,
    chars: { ...shared.chars, ...drafted?.chars },
  }
  const chars = style.chars ?? {}
  const mixed = new Set(shared.mixed.filter((key) => drafted?.chars?.[key] === undefined))
  const frame = lettering?.frame ?? 'point'
  // A box and a path fit their text alike, a balloon's translation in a path; point text has no fit.
  const fitted = typeof frame !== 'object' ? null : frame.box ? 'box' : 'path'
  const fit = typeof frame === 'object' ? (frame.box ?? frame.path)?.fit : undefined
  const autoFit = fit === 'best'
  // Booharu: what the targets' paragraphs share; a value they don't shows empty, unless the draft
  // sets it, which every target then takes (S11-h).
  const share = useMemo(() => paragraphShare(targets), [targets])
  const fitMixed = share.fit.mixed

  /** Merges `patch` into each target's style, shown at once on the first. */
  // Booharu: or gives each target its own patch from `each`, in one edit, shown by `patch`, what
  // every target takes (S11-h).
  const apply = (patch: TextStyle, each?: (layer: Layer) => TextStyle | null) => {
    if (!current) return
    restyle.mutate({
      layer: current.id,
      // Booharu: only what's set, so a value the targets don't share stays empty (S11-h).
      style: { ...drafted, ...patch, chars: { ...drafted?.chars, ...patch.chars } },
      targets,
      patch,
      each, // Booharu: each target's own (S11-h).
    })
  }

  /** A character setting: while typing, the canvas styles the selected characters, or holds it
   * for the next ones typed at a caret; otherwise every character of each target takes it. */
  const applyChars = (patch: CharStyle, words?: WordChange) => {
    // Booharu: or the words chosen in Story take it, or what `words` asks, through the words edit.
    if (!typing && !chosen) return apply({ chars: patch })
    // A mark can't unset a value, so no border on some characters is a border no wide.
    // Booharu: the same for the second border, so `marked` is a let.
    let marked: CharStyle =
      patch.stroke === null ? { ...patch, stroke: undefined, stroke_width: 0 } : patch
    if (marked.outer_stroke === null) {
      marked = { ...marked, outer_stroke: undefined, outer_stroke_width: 0 }
    }
    if (chosen) return void styleWords(chosen, words ?? { set: marked }) // Booharu: words chosen in Story.
    send(commands.styleTyping, marked)
  }

  /** Merges `patch` into each target's lettering: a text's own values, a block's translation's. */
  const update = (patch: { [key in string]: Value }) => {
    for (const layer of targets) {
      send(updateLayer, layer.id, isBlockLayer(layer) ? { translation: patch } : patch)
    }
  }

  /** Booharu: a border's colour or width, `patch`, with `companion`, the width or colour that goes
   * with it (S11-h). Several boxes take it in one edit, each keeping its own companion, the bar's
   * default only where one has none; while typing, for words chosen in Story or for one box, the
   * characters take `own` with it, unless their companions differ, which they keep. */
  const applyBorder = (patch: CharStyle, companion: BorderKey, own: CharStyle) => {
    if (!typing && !chosen && targets.length > 1) {
      return apply({ chars: patch }, (layer) => ({
        chars: { ...patch, ...borderCompanion(layer, companion) },
      }))
    }
    applyChars(mixed.has(companion) ? patch : { ...patch, ...own })
  }

  const families = useMemo(() => {
    const available = availableFonts ?? []
    return available.some((family) => sameFont(family.name, defaultFont))
      ? available
      : [...available, placeholderFamily(defaultFont)]
  }, [availableFonts, defaultFont])
  const family = mixed.has('family') ? '' : (chars.family ?? defaultFont)
  const selectedFamily = families.find((candidate) => sameFont(candidate.name, family))
  // Booharu: italic as the characters have it, on every font, as the renderer slants a family with
  // no italic face (D12), by the rule Ctrl+I and the Story panel share.
  const italic = isItalic(chars, mixed)
  const weight = chars.weight ?? 400
  // Booharu: the family's own weights of either style, with regular, bold and the weight shown, none
  // snapped to a face, as the renderer makes a weight a family lacks (D12).
  const weights = weightChoices(
    [...usableFontWeights(selectedFamily, false), ...usableFontWeights(selectedFamily, true)],
    weight,
  )
  const size = mixed.has('size') ? null : Math.round((chars.size ?? 24) * 100) / 100
  // Booharu: the colour, or the gradient, every character shows, none while they differ (S12-c).
  const paint = sharedPaint(chars, mixed)
  const strokeWidth = chars.stroke_width ?? 0
  const stroke = chars.stroke ?? null
  const strokeEnabled = stroke !== null && stroke[3] > 0 && strokeWidth > 0
  // Booharu: the second border, which only a first border has: each character's own, whatever its
  // colour or width. While typing, the canvas doesn't say, so differing borders count.
  const bordered =
    strokeEnabled ||
    ((mixed.has('stroke') || mixed.has('stroke_width')) &&
      ('bordered' in shared ? shared.bordered : true))
  // Booharu: how the borders are drawn, none shown while the targets' methods differ (S11-h).
  const strokeMethod =
    share.stroke_method.mixed && drafted?.stroke_method === undefined
      ? null
      : (style.stroke_method ?? 'expansion')
  const direction: Direction = lettering?.direction ?? 'horizontal'
  // Booharu: none pressed while the targets' alignments differ (S11-h).
  const alignMixed = share.align.mixed && drafted?.align === undefined
  const align: Align | null = alignMixed
    ? null
    : (style.align ?? (direction === 'vertical' ? 'start' : 'center'))

  // The bar sits by the selection as the canvas lays it out, a point text's lines and the text
  // being typed included.
  const selectionBounds = useKoharuStore((state) => state.selectionBounds)
  // A new text being typed has no layer yet for its paragraph to go to.
  const paragraph = targets.length > 0
  // Booharu: Text effects goes with the bar, which shows as the line below says (S12-k).
  useFxBar(
    page?.id,
    selectedIds,
    !!selectionBounds && !!(targets.length || typing) && TOOLS.includes(tool),
  )
  if (!selectionBounds || !(targets.length || typing) || !TOOLS.includes(tool)) return null

  // One row, as Photoshop's type options bar, at the user's choice: the face, its size, and its
  // colour, then the paragraph; italic and vertical text are toggles, and the border's colour and
  // width close the row, as Clip Studio Paint's text tool shows a text's edge beside its colour.
  const toggle =
    'h-6 w-6 min-w-6 rounded-md bg-background px-0 text-muted-foreground data-[pressed]:border-primary data-[pressed]:bg-primary data-[pressed]:text-primary-foreground data-[pressed]:hover:bg-primary/90'
  return (
    <ContextBar
      bounds={selectionBounds}
      aria-label={t('inspector.type')}
      data-testid='type-bar'
      className='flex-wrap'
      onPointerDown={keepWords} // Booharu: a press here, or in its popups, keeps words chosen in Story.
    >
      {/* Booharu: what the character settings apply to, with Whole box (S11-f). */}
      <AppliesTo words={chosen} />
      <div className='w-28'>
        <FontPicker
          value={family}
          families={families}
          size='sm'
          {...booharuFonts} // Booharu: favourites and local names.
          // Booharu: only the family, so B and I stay on, on the box and on single words, and a font
          // change resets only the words' families (D22).
          onChange={(next) => applyChars({ family: next })}
        />
      </div>
      <Choice
        size='sm'
        value={mixed.has('weight') ? '' : String(weight)}
        choices={Object.fromEntries(weights.map((value) => [value, String(value)]))}
        label={t('inspector.fontWeight')}
        className='w-14'
        onChange={(next) => applyChars({ weight: Number(next) })}
      />
      {/* Booharu: B beside the weights, pressed while every character is bold (D12). */}
      <BoldToggle chars={chars} mixed={mixed} className={toggle} onChange={applyChars} />
      <Tooltip>
        <TooltipTrigger
          render={
            <Toggle
              variant='outline'
              size='sm'
              aria-label={t('inspector.fontStyles.italic')}
              className={toggle}
              pressed={italic}
              // Booharu: never disabled, and only italic, as the renderer slants a family with no
              // italic face (D12); for words chosen in Story, as the word toolbar's I; off while
              // typing takes the italic off the characters for real (S11-d).
              onPressedChange={(nextItalic) =>
                typing && italic
                  ? unmarkTyping('italic')
                  : applyChars({ italic: nextItalic }, italicWords(italic))
              }
            />
          }
        >
          <Italic className='size-3.5' />
        </TooltipTrigger>
        <TooltipContent>
          {/* Booharu: or that the characters' italics differ, which a press turns on (S11-h). */}
          {mixed.has('italic')
            ? t('booharu.typebar.mixedToggle', { field: t('inspector.fontStyles.italic') })
            : t('inspector.fontStyles.italic')}
        </TooltipContent>
      </Tooltip>
      {/* Booharu: the star, which stresses words typed or chosen in Story, never a whole box. */}
      <EmphasisButton words={chosen} />
      {/* Booharu: Clear, which takes the word styles off (S11-g). */}
      <ClearWordStyles words={chosen} targets={targets} />
      <div className='w-15'>
        <PresetField
          name='font-size'
          testId='type-size'
          // Booharu: empty, "Mixed", and neither Auto nor a size chosen while the boxes' fits differ;
          // "Mixed" too while their sizes do (S11-h).
          value={fitMixed ? null : size}
          mixed={fitMixed || size === null ? t('booharu.typebar.mixed') : undefined}
          min={0.5}
          // Photoshop's largest type size.
          max={1296}
          presets={fontSizes}
          label={t('inspector.fontSize')}
          menuLabel={t('inspector.chooseFontSize')}
          auto={{
            label: t('inspector.auto'),
            active: autoFit && !fitMixed, // Booharu: not while the fits differ (S11-h).
            available: fitted !== null,
            onPick: () => fitted && update({ frame: { [fitted]: { fit: 'best' } } }),
          }}
          onChange={(next) => {
            applyChars({ size: next })
            // A typed size is the size the text draws at, as in Photoshop and Clip Studio Paint, so
            // a frame that shrinks or grows its text stops fitting it; a growing box keeps the size.
            if (fitted && fit !== 'none' && fit !== 'grow') {
              update({ frame: { [fitted]: { fit: 'none' } } })
            }
          }}
        />
      </div>
      {/* Booharu: A− and A+, for the characters typed or the words chosen in Story (S11-e). */}
      <WordSizeSteps words={chosen} />
      <ColorWell
        label={t('inspector.textColor')}
        size='sm'
        // Booharu: empty too while a gradient shows, and a colour picked takes the gradient off,
        // a fill and a gradient fill being one paint (S12-c).
        value={!paint || 'gradient' in paint ? '' : chars.fill ? toHex(chars.fill) : BLACK}
        onChange={(color) => color && applyChars({ fill: toRgba(color), fill_gradient: null })}
      />
      <div
        role='group'
        aria-label={t('inspector.alignment')}
        className='grid h-6 w-[4.5rem] grid-cols-3 rounded-md border border-input bg-background p-px'
      >
        {(
          [
            [
              'start',
              AlignLeft,
              direction === 'vertical' ? t('inspector.alignTop') : t('inspector.alignLeft'),
            ],
            [
              'center',
              AlignCenter,
              direction === 'vertical' ? t('inspector.alignMiddle') : t('inspector.alignCenter'),
            ],
            [
              'end',
              AlignRight,
              direction === 'vertical' ? t('inspector.alignBottom') : t('inspector.alignRight'),
            ],
          ] as const
        ).map(([value, Icon, label]) => (
          // Booharu: the tooltip adds "Mixed" while the targets' alignments differ (S11-h).
          <Tooltip key={value}>
            <TooltipTrigger
              render={
                <button
                  type='button'
                  aria-label={label}
                  aria-pressed={align === value}
                  data-active={align === value}
                  disabled={!paragraph}
                  className='grid place-items-center rounded-[4px] text-muted-foreground hover:text-foreground data-[active=true]:bg-primary data-[active=true]:text-primary-foreground data-[active=true]:hover:bg-primary/90'
                  onClick={() => apply({ align: value })}
                />
              }
            >
              <Icon className='size-3' />
            </TooltipTrigger>
            <TooltipContent>
              {label}
              {alignMixed && <span className='ml-2 opacity-60'>{t('booharu.typebar.mixed')}</span>}
              {/* Booharu: the whole box's, while typing or with words chosen in Story (S11-f). */}
              <WholeBoxOnly words={chosen} />
            </TooltipContent>
          </Tooltip>
        ))}
      </div>
      {/* Booharu: spacing: line spacing, which only the paragraph has (S11-i), the whole box's
          (S11-f), and letter spacing, at the bar's scope (S12-k). */}
      <Spacing
        lineHeight={style.line_height ?? null}
        lineHeightMixed={share.line_height.mixed && drafted?.line_height === undefined}
        paragraph={paragraph}
        tracking={chars.tracking ?? null}
        trackingMixed={mixed.has('tracking')}
        words={chosen}
        onLineHeight={(lineHeight) => apply({ line_height: lineHeight })}
        onTracking={(tracking) => applyChars({ tracking })}
      />
      <Tooltip>
        <TooltipTrigger
          render={
            <Toggle
              variant='outline'
              size='sm'
              aria-label={t('inspector.vertical')}
              className={toggle}
              disabled={!paragraph}
              // Booharu: unpressed while the targets' directions differ, so a press makes every
              // one vertical (S11-h).
              pressed={!share.vertical.mixed && direction === 'vertical'}
              onPressedChange={(vertical) =>
                update({ direction: vertical ? 'vertical' : 'horizontal' })
              }
            />
          }
        >
          {/* Columns of text read from the right, as vertical text sets. */}
          <svg
            aria-hidden='true'
            viewBox='0 0 24 24'
            fill='none'
            stroke='currentColor'
            strokeWidth={2}
            strokeLinecap='round'
            className='size-3.5'
          >
            <path d='M18 4v16M12 4v11M6 4v7' />
          </svg>
        </TooltipTrigger>
        <TooltipContent>
          {share.vertical.mixed ? ( // Booharu: or that the directions differ (S11-h).
            t('booharu.typebar.mixedDirection')
          ) : (
            <>
              {t('inspector.textDirection')}
              <span className='ml-2 opacity-60'>{t(`inspector.${direction}`)}</span>
            </>
          )}
          {/* Booharu: the whole box's, while typing or with words chosen in Story (S11-f). */}
          <WholeBoxOnly words={chosen} />
        </TooltipContent>
      </Tooltip>
      <div role='group' aria-label={t('inspector.border')} className='flex items-center gap-1'>
        <ColorWell
          label={t('inspector.borderColor')}
          size='sm'
          allowTransparent
          value={
            // Booharu: empty while the colours differ, and the colour shared while only the widths
            // do (S11-h).
            mixed.has('stroke')
              ? ''
              : stroke && stroke[3] > 0 && (strokeWidth > 0 || mixed.has('stroke_width'))
                ? toHex(stroke)
                : null
          }
          onChange={(color) =>
            color === null
              ? applyChars({ stroke: null })
              : // Booharu: through applyBorder, so each box keeps its own width (S11-h).
                applyBorder({ stroke: toRgba(color) }, 'stroke_width', {
                  stroke_width: strokeWidth > 0 ? strokeWidth : 1.5,
                })
          }
        />
        {/* Booharu: as wide as the size field, not Koharu's w-12, so "Mixed" fits (V1). */}
        <div className='w-15'>
          <PresetField
            name='border-width'
            // Booharu: empty, "Mixed", while the widths differ (S11-h).
            value={mixed.has('stroke_width') ? null : strokeWidth > 0 ? strokeWidth : 1.5}
            mixed={mixed.has('stroke_width') ? t('booharu.typebar.mixed') : undefined}
            min={0.5}
            max={32}
            presets={borderWidths}
            label={t('inspector.borderWidth')}
            menuLabel={t('inspector.chooseBorderWidth')}
            onChange={(next) =>
              // Booharu: through applyBorder, so each box keeps its own colour (S11-h).
              applyBorder({ stroke_width: next }, 'stroke', { stroke: stroke ?? toRgba(WHITE) })
            }
          />
        </div>
        {/* Booharu: the second border, the colour swap and the outline method, behind a small
            button beside the border (Q2). */}
        <BorderExtras
          chars={chars}
          mixed={mixed}
          bordered={bordered}
          method={strokeMethod}
          paragraph={paragraph}
          words={chosen}
          onChars={applyChars}
          onBorder={applyBorder}
          onStyle={apply}
        />
      </div>
      {/* Booharu: Text effects, after the border's and before Done (S12-k). */}
      <FxPopover
        targets={targets}
        typing={typing}
        words={chosen}
        chars={chars}
        mixed={mixed}
        onChars={applyChars}
        onEach={apply}
      />
      {/* Booharu: Done, last, while the canvas types (S11-c). */}
      <DoneButton />
    </ContextBar>
  )
}

/** A family the font list hasn't loaded, so the default still shows. */
function placeholderFamily(name: string): FontFamily {
  return {
    name,
    faces: [{ post_script_name: name, weight: 400, weight_range: null, italic: false }],
  }
}

const CHAR_KEYS: CharKey[] = [
  'family',
  'weight',
  'italic',
  'size',
  'fill',
  'stroke',
  'stroke_width',
  // Booharu: the second border's keys.
  'outer_stroke',
  'outer_stroke_width',
  // Booharu: the gradient fill and letter spacing (S12-c).
  'fill_gradient',
  'tracking',
]

// Booharu: exported for its test, and it says whether every character has a border.
/** What every character of `texts` shares, each span's marks over its text's style, and the values
 * they don't, which it leaves empty, as the canvas reports a typed selection's; and, for Booharu's
 * second border, whether every character has a border of its own. */
export function sharedChars(texts: Text[]): {
  chars: CharStyle
  mixed: CharKey[]
  bordered: boolean
} {
  const styles: CharStyle[] = texts.flatMap((text) => {
    const base = text.style?.chars ?? {}
    const spans = text.content?.length ? text.content : [{ insert: '' }]
    // Booharu: merged as koharu-scene's CharStyle::or merges them, a fill and a gradient fill
    // being one paint (S12-c).
    return spans.map((span) => charsOr(span.attributes ?? {}, base))
  })
  const [first, ...rest] = styles
  if (!first) return { chars: {}, mixed: [], bordered: false }
  const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null)
  const mixed = CHAR_KEYS.filter((key) => rest.some((style) => !same(style[key], first[key])))
  const chars: CharStyle = { ...first }
  for (const key of mixed) delete chars[key]
  const bordered = styles.every(
    (style) => (style.stroke?.[3] ?? 0) > 0 && (style.stroke_width ?? 0) > 0,
  )
  return { chars, mixed, bordered }
}

// Booharu: no hasItalic, as italic shows on every font (D12).

function usableFontWeights(family: FontFamily | undefined, italic: boolean): number[] {
  if (!family) return [400]
  const styled = family.faces.filter((face) => face.italic === italic)
  const faces = styled.length ? styled : family.faces
  const weights = new Set(faces.map((face) => face.weight))
  for (const face of faces) {
    if (!face.weight_range) continue
    const { start, end } = face.weight_range
    weights.add(start)
    weights.add(end)
    for (let weight = 100; weight <= 900; weight += 100) {
      if (weight >= start && weight <= end) weights.add(weight)
    }
  }
  return [...weights].sort((left, right) => left - right)
}

// Booharu: no nearestFontWeight, as no weight is snapped to a face (D12).

function sameFont(left: string, right: string): boolean {
  const normalize = (value: string) =>
    value.normalize('NFKC').trim().replace(/\s+/g, ' ').toLowerCase()
  return normalize(left) === normalize(right)
}

/** The font sizes the size field lists, as Photoshop's options bar does. */
const fontSizes = [8, 9, 10, 12, 14, 16, 18, 20, 24, 28, 32, 36, 48, 64, 72, 96]
/** The border widths the width field lists. */
const borderWidths = [1, 2, 3, 4, 5, 6, 8, 10, 12, 16, 20, 24, 32]

/** A number typed in, in half steps, or picked from `presets` behind the arrow beside it, as
 * Photoshop's options bar sets a font size. A typed number applies on Enter or on leaving the
 * field, and one outside `min` to `max` goes back. `auto` is a choice above the presets that the
 * field shows by its label. */
function PresetField({
  name,
  value,
  min,
  max,
  presets,
  label,
  menuLabel,
  auto,
  mixed, // Booharu: "Mixed" (S11-h).
  testId,
  onChange,
}: {
  name: string
  /** `null` when the selection doesn't share one. */
  value: number | null
  min: number
  max: number
  presets: number[]
  label: string
  menuLabel: string
  auto?: { label: string; active: boolean; available: boolean; onPick: () => void }
  /** Booharu: what the empty field shows while the selection doesn't share a value (S11-h). */
  mixed?: string
  testId?: string
  onChange: (value: number) => void
}) {
  const id = useId()
  const field = useRef<HTMLDivElement>(null)
  const shown = auto?.active ? null : value
  const [draft, setDraft] = useState<number | null>(shown)

  useEffect(() => {
    setDraft(shown)
  }, [shown])

  const pick = (choice: string) => {
    if (choice === 'auto') {
      setDraft(null)
      auto?.onPick()
      return
    }
    const preset = Number(choice)
    setDraft(preset)
    onChange(preset)
  }

  // The chosen one shows as a select's chosen item does, its tick left out, which the field's
  // narrow list has no room for.
  const item =
    'pr-2 data-checked:bg-accent data-checked:text-accent-foreground [&>[data-slot=dropdown-menu-radio-item-indicator]]:hidden'

  const commit = (next: number | null) => {
    if (next !== null && next >= min && next <= max) onChange(next)
    else setDraft(shown)
  }

  return (
    <NumberField
      id={id}
      name={name}
      className='min-w-0'
      value={draft}
      min={min}
      max={max}
      step={0.5}
      onValueChange={setDraft}
      onValueCommitted={commit}
    >
      <Tooltip>
        <TooltipTrigger render={<NumberFieldGroup ref={field} />}>
          <NumberFieldInput
            data-testid={testId}
            aria-label={label}
            placeholder={auto?.active ? auto.label : mixed /* Booharu: S11-h. */}
            className='px-1 text-left text-ellipsis'
          />
          <DropdownMenu>
            <DropdownMenuTrigger
              aria-label={menuLabel}
              className='grid h-full w-4 shrink-0 place-items-center border-l border-input text-muted-foreground outline-none hover:bg-muted hover:text-foreground focus-visible:bg-muted focus-visible:text-foreground'
            >
              <ChevronDown className='size-3' />
            </DropdownMenuTrigger>
            {/* Under the whole field and as wide, as a select's list opens, wider only for a
              label that needs it. */}
            <DropdownMenuContent anchor={field} align='start' className='min-w-fit'>
              <DropdownMenuRadioGroup
                value={auto?.active ? 'auto' : String(value)}
                onValueChange={pick}
              >
                {auto?.available && (
                  <DropdownMenuRadioItem value='auto' className={item}>
                    {auto.label}
                  </DropdownMenuRadioItem>
                )}
                {presets.map((preset) => (
                  <DropdownMenuRadioItem
                    key={preset}
                    value={String(preset)}
                    className={cn(item, 'tabular-nums')}
                  >
                    {preset}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        </TooltipTrigger>
        <TooltipContent>{label}</TooltipContent>
      </Tooltip>
    </NumberField>
  )
}
