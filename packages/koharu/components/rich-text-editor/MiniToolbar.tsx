'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import { useFonts } from '@/lib/queries'
import type { RunStylePatch, StyleRun, Typography } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'

import { selectedStyles, stylePatch } from './model'

export function MiniToolbar({
  runs,
  start,
  end,
  typography,
  baseSize,
  busy,
  apply,
  emphasis,
  clear,
  done,
}: {
  runs: StyleRun[]
  start: number
  end: number
  typography: Typography | null
  baseSize: number | null
  busy: boolean
  apply: (patch: RunStylePatch) => void
  emphasis: () => void
  clear: () => void
  done: () => void
}) {
  const { t } = useTranslation()
  const fonts = useFonts().data ?? []
  const values = selectedStyles(runs, start, end, typography)
  const px =
    baseSize !== null && values.scale !== null
      ? Math.round(baseSize * values.scale * 100) / 100
      : null
  const [draft, setDraft] = useState<string | null>(null)
  const resize = (direction: number) => {
    const scale = values.scale ?? 1
    const size_scale =
      baseSize === null
        ? Math.max(0.1, direction > 0 ? scale * 1.05 : scale / 1.05)
        : Math.max(0.1, (px ?? baseSize) + direction) / baseSize
    apply(stylePatch({ size_scale }))
  }
  const commitSize = () => {
    const next = draft === null ? px : Number(draft)
    setDraft(null)
    if (draft !== null && baseSize !== null && next !== null && Number.isFinite(next) && next > 0)
      apply(stylePatch({ size_scale: next / baseSize }))
  }
  return (
    <div
      data-canvas-control
      role='toolbar'
      aria-label={t('richText.toolbar')}
      className='flex max-w-[calc(100vw-2rem)] flex-col gap-1 rounded-lg border border-border bg-popover p-1 text-popover-foreground shadow-lg'
    >
      {/* Two short rows instead of one long strip: font and size, then style and actions. */}
      <div data-canvas-control className='flex items-center gap-1'>
        <div className='w-36' data-canvas-control>
          <FontPicker
            size='sm'
            ariaLabel={t('richText.font')}
            value={values.font ?? ''}
            placeholder={t('richText.mixed')}
            families={fonts}
            disabled={busy}
            onChange={(font) => apply(stylePatch({ font }))}
          />
        </div>
        <Button
          data-canvas-control
          size='icon-xs'
          variant='ghost'
          disabled={busy}
          aria-label={t('richText.smaller')}
          title={baseSize === null ? t('richText.autoFitSmaller') : t('richText.smaller')}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => resize(-1)}
        >
          A−
        </Button>
        <Input
          data-canvas-control
          type='number'
          min={0.1}
          step={1}
          className='h-6 w-16 px-1 text-center text-[11px]'
          aria-label={t('richText.size')}
          title={baseSize === null ? t('richText.autoFitSizeUnavailable') : t('richText.size')}
          placeholder={baseSize === null ? '—' : t('richText.mixed')}
          disabled={busy || baseSize === null}
          value={draft ?? px ?? ''}
          onChange={(event) => setDraft(event.currentTarget.value)}
          onBlur={commitSize}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault()
              commitSize()
            }
            if (event.key === 'Escape') setDraft(null)
          }}
        />
        <Button
          data-canvas-control
          size='icon-xs'
          variant='ghost'
          disabled={busy}
          aria-label={t('richText.bigger')}
          title={baseSize === null ? t('richText.autoFitBigger') : t('richText.bigger')}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => resize(1)}
        >
          A+
        </Button>
      </div>
      <div data-canvas-control className='flex items-center gap-1'>
        <Button
          data-canvas-control
          size='icon-xs'
          variant='ghost'
          disabled={busy}
          className='font-bold aria-pressed:bg-primary aria-pressed:text-primary-foreground'
          aria-label={t('richText.bold')}
          aria-pressed={values.weight === null ? 'mixed' : values.weight >= 600}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() =>
            apply(
              values.weight !== null && values.weight >= 600
                ? stylePatch({}, ['font_weight'])
                : stylePatch({ font_weight: 700 }),
            )
          }
        >
          B
        </Button>
        <Button
          data-canvas-control
          size='icon-xs'
          variant='ghost'
          disabled={busy}
          className='font-serif italic aria-pressed:bg-primary aria-pressed:text-primary-foreground'
          aria-label={t('richText.italic')}
          aria-pressed={values.italic === null ? 'mixed' : values.italic === 'italic'}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() =>
            apply(
              values.italic === 'italic'
                ? stylePatch({}, ['font_style'])
                : stylePatch({ font_style: 'italic' }),
            )
          }
        >
          I
        </Button>
        <div
          data-canvas-control
          title={values.color === null ? t('richText.mixed') : t('richText.color')}
        >
          <ColorWell
            size='sm'
            label={values.color === null ? t('richText.mixedColor') : t('richText.color')}
            disabled={busy}
            value={rgbaToHex((values.color ?? [0, 0, 0, 255]) as [number, number, number, number])}
            onChange={(color) => apply(stylePatch({ color: hexToRgba(color) }))}
          />
        </div>
        <Button
          data-canvas-control
          size='icon-xs'
          variant='ghost'
          disabled={busy}
          aria-label={t('richText.emphasis')}
          title={t('richText.emphasis')}
          onMouseDown={(event) => event.preventDefault()}
          onClick={emphasis}
        >
          ★
        </Button>
        <Button
          data-canvas-control
          size='xs'
          variant='ghost'
          disabled={busy}
          onMouseDown={(event) => event.preventDefault()}
          onClick={clear}
        >
          {t('richText.clear')}
        </Button>
        <Button
          data-canvas-control
          size='xs'
          variant='secondary'
          onMouseDown={(event) => event.preventDefault()}
          onClick={done}
        >
          {t('richText.done')}
        </Button>
      </div>
    </div>
  )
}
