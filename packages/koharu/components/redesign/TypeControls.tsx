'use client'

import { AlignCenter, AlignLeft, AlignRight } from 'lucide-react'
import { useMemo } from 'react'
import { useTranslation } from 'react-i18next'

import { ColorWell } from '@/components/controls/ColorWell'
import { FontPicker } from '@/components/controls/FontPicker'
import { GradientControls } from '@/components/text-extras/GradientControls'
import { SpacingControls } from '@/components/text-extras/SpacingControls'
import { hexToRgba, rgbaToHex } from '@/lib/color'
import { expandLayerSelection, isTextLayer } from '@/lib/document'
import { useFonts, usePage } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import type { Typography } from '@koharu/bridge/protocol'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@koharu/ui/components/tabs'

import { Field, OutlineControls, SelectControl, type TypographyControlsProps } from './Controls'
import { FontSizeField } from './FontSizeField'
import { useRedesignStore, type TypeTab } from './state'
import {
  defaultFont,
  defaultTypography,
  findFontFamily,
  nearestFontWeight,
  normalizeFontName,
  styledFontWeight,
  usableFontWeights,
} from './typography'
import { isMixed, useTypography } from './useTypography'

export function TypeInspector() {
  const page = usePage().data
  const selectedIds = useKoharuStore((state) => state.selectedLayers)
  const expanded = page ? expandLayerSelection(page.layers, selectedIds) : []
  const targets =
    page?.layers
      .filter(isTextLayer)
      .filter((layer) => expanded.includes(layer.id))
      .map((layer) => layer.id) ?? []
  return <TypePanel key={JSON.stringify([page?.id, targets])} targets={targets} />
}

function TypePanel({ targets }: { targets: string[] }) {
  const { t } = useTranslation()
  const tab = useRedesignStore((state) => state.tab)
  const setTab = useRedesignStore((state) => state.setTab)
  const data = useTypography(targets)
  return (
    <div className='min-w-0 p-2' data-testid='type-inspector' aria-disabled={data.disabled}>
      <Tabs value={tab} onValueChange={(next) => setTab(next as TypeTab)}>
        <TabsList className='h-7 w-full' aria-label={t('inspector.type')}>
          {(['text', 'outline', 'fill'] as const).map((value) => (
            <TabsTrigger key={value} className='text-[10px]' value={value}>
              {t(`redesign.tabs.${value}`)}
            </TabsTrigger>
          ))}
        </TabsList>
        <TabsContent value='text'>{tab === 'text' && <TextControls {...data} />}</TabsContent>
        <TabsContent value='outline'>
          {tab === 'outline' && <OutlineControls {...data} />}
        </TabsContent>
        <TabsContent value='fill'>{tab === 'fill' && <GradientControls {...data} />}</TabsContent>
      </Tabs>
    </div>
  )
}

function TextControls({ typography, values, disabled, apply }: TypographyControlsProps) {
  const { t } = useTranslation()
  const availableFonts = useFonts().data
  const families = useMemo(() => {
    const available = availableFonts ?? []
    return available.some(
      (family) => normalizeFontName(family.name) === normalizeFontName(defaultFont.name),
    )
      ? available
      : [...available, defaultFont]
  }, [availableFonts])
  const mixed = (key: keyof Typography) => isMixed(values, (value) => value[key])
  const familyFor = (value: Typography) =>
    findFontFamily(families, value.preferred_font ?? defaultFont.name)
  const style = typography.font_style ?? 'normal'
  const weight = typography.font_weight ?? 400
  const weights = usableFontWeights(familyFor(typography), style)
  const bold = (value: Typography) => (value.font_weight ?? 400) >= 600
  const italic = (value: Typography) => value.font_style === 'italic'
  const boldMixed = isMixed(values, bold)
  const italicMixed = isMixed(values, italic)
  const writingMode = typography.writing_mode ?? 'Horizontal'
  const alignmentFor = (value: Typography) =>
    value.alignment ?? (value.writing_mode === 'Vertical' ? 'Start' : 'Center')
  return (
    <div className='grid min-w-0 gap-1.5'>
      <div className='grid min-w-0 grid-cols-[minmax(0,1fr)_3rem] gap-1.5'>
        <Field label={t('inspector.font')}>
          <FontPicker
            size='sm'
            disabled={disabled}
            value={mixed('preferred_font') ? '' : (typography.preferred_font ?? defaultFont.name)}
            placeholder={t('redesign.mixed')}
            families={families}
            onChange={(preferred_font) =>
              apply((value) => {
                // Bold and italic survive a font change, synthesized when the face is missing.
                const font_style = value.font_style ?? 'normal'
                return {
                  ...value,
                  preferred_font,
                  font_style,
                  font_weight: styledFontWeight(
                    findFontFamily(families, preferred_font),
                    font_style,
                    value.font_weight ?? 400,
                  ),
                }
              })
            }
          />
        </Field>
        <Field label={mixed('color') ? t('redesign.mixed') : t('inspector.color')}>
          <ColorWell
            label={t('inspector.textColor')}
            size='sm'
            disabled={disabled || values.some((value) => !!value.fill_gradient)}
            value={rgbaToHex(typography.color ?? defaultTypography.color!)}
            onChange={(color) => apply((value) => ({ ...value, color: hexToRgba(color) }))}
          />
        </Field>
      </div>
      <div className='grid min-w-0 grid-cols-[minmax(0,1fr)_4.25rem_3.25rem] gap-1.5'>
        <Field label={t('inspector.size')}>
          <FontSizeField
            value={Math.round((typography.size ?? 24) * 100) / 100}
            autoFit={typography.auto_fit}
            mixed={mixed('size') || mixed('auto_fit')}
            disabled={disabled}
            onChange={(size) => apply((value) => ({ ...value, size, auto_fit: false }))}
            onAutoFit={() => apply((value) => ({ ...value, auto_fit: true }))}
          />
        </Field>
        <Field label={t('inspector.weight')}>
          <SelectControl
            label={t('inspector.fontWeight')}
            value={mixed('font_weight') ? null : String(weight)}
            disabled={disabled}
            options={[...new Set([...weights, weight])]
              .sort((a, b) => a - b)
              .map((value) => [String(value), String(value)])}
            onChange={(weight) =>
              apply((value) => ({
                ...value,
                font_weight: nearestFontWeight(
                  usableFontWeights(familyFor(value), value.font_style ?? 'normal'),
                  Number(weight),
                ),
              }))
            }
          />
        </Field>
        <Field label={t('inspector.style')}>
          <div className='flex h-6 gap-0.5'>
            {(['bold', 'italic'] as const).map((kind) => {
              const mixed = kind === 'bold' ? boldMixed : italicMixed
              const active = !mixed && (kind === 'bold' ? bold(typography) : italic(typography))
              const label = t(`redesign.${kind}`)
              return (
                <button
                  key={kind}
                  type='button'
                  aria-label={label}
                  aria-pressed={active}
                  title={mixed ? t('redesign.mixedToggle', { field: label }) : label}
                  disabled={disabled}
                  data-mixed={mixed}
                  className='grid size-6 place-items-center rounded-sm border border-input text-[11px] disabled:opacity-40 aria-pressed:bg-primary aria-pressed:text-primary-foreground'
                  onClick={() =>
                    apply((value) => {
                      // A family without a bold or italic face still gets the style:
                      // the renderer draws synthetic bold and slant for missing faces.
                      if (kind === 'bold')
                        return {
                          ...value,
                          font_weight: styledFontWeight(
                            familyFor(value),
                            value.font_style ?? 'normal',
                            active ? 400 : 700,
                          ),
                        }
                      const font_style = active ? 'normal' : 'italic'
                      return {
                        ...value,
                        font_style,
                        font_weight: styledFontWeight(
                          familyFor(value),
                          font_style,
                          value.font_weight ?? 400,
                        ),
                      }
                    })
                  }
                >
                  {kind === 'bold' ? <b>B</b> : <i>I</i>}
                </button>
              )
            })}
          </div>
        </Field>
      </div>
      <div className='grid min-w-0 grid-cols-2 gap-1.5'>
        <Field label={t('inspector.alignment')}>
          <div className='grid h-6 grid-cols-3 rounded-md border border-input p-px'>
            {(['Start', 'Center', 'End'] as const).map((alignment, index) => {
              const Icon = [AlignLeft, AlignCenter, AlignRight][index]
              const label = t(
                writingMode === 'Vertical'
                  ? ['inspector.alignTop', 'inspector.alignMiddle', 'inspector.alignBottom'][index]
                  : ['inspector.alignLeft', 'inspector.alignCenter', 'inspector.alignRight'][index],
              )
              return (
                <button
                  key={alignment}
                  type='button'
                  aria-label={label}
                  title={isMixed(values, alignmentFor) ? t('redesign.mixed') : label}
                  aria-pressed={
                    !isMixed(values, alignmentFor) && alignmentFor(typography) === alignment
                  }
                  disabled={disabled}
                  className='grid place-items-center rounded-sm text-muted-foreground disabled:opacity-40 aria-pressed:bg-primary aria-pressed:text-primary-foreground'
                  onClick={() => apply((value) => ({ ...value, alignment }))}
                >
                  <Icon className='size-3' />
                </button>
              )
            })}
          </div>
        </Field>
        <Field label={t('inspector.direction')}>
          <SelectControl
            label={t('inspector.textDirection')}
            disabled={disabled}
            value={mixed('writing_mode') ? null : (typography.writing_mode ?? 'Auto')}
            options={[
              ['Auto', t('inspector.auto')],
              ['Horizontal', t('inspector.horizontal')],
              ['Vertical', t('inspector.vertical')],
            ]}
            onChange={(writing_mode) =>
              apply((value) => ({
                ...value,
                writing_mode:
                  writing_mode === 'Auto' ? null : (writing_mode as Typography['writing_mode']),
              }))
            }
          />
        </Field>
      </div>
      <SpacingControls typography={typography} values={values} disabled={disabled} apply={apply} />
      <p className='text-[9px] leading-4 text-muted-foreground'>{t('redesign.wheelHint')}</p>
    </div>
  )
}
