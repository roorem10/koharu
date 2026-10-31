'use client'

import { useEffect, useId, useRef, useState, type ReactNode } from 'react'
import { useTranslation } from 'react-i18next'

import { discussLineEvent } from '@/components/context/events'
import { GlossaryTextMenu } from '@/components/context/GlossaryTextMenu'
import { CommitTextarea } from '@/components/controls/CommitTextarea'
import { BoxActions } from '@/components/editor/BoxActions'
import { BoxNoteLine } from '@/components/editor/BoxNoteLine'
import { BubbleShapeLine } from '@/components/editor/BubbleShapeLine'
import { LetteringCategoryLine } from '@/components/editor/LetteringCategoryLine'
import { LineContextCard } from '@/components/editor/LineContextCard'
import { BoxRedoSection } from '@/components/ocr-box/BoxRedoSection'
import { BoxQcSection } from '@/components/qc/BoxQcSection'
import { useBoxIssues } from '@/components/qc/useBoxIssues'
import { gradientFor, GradientControls } from '@/components/text-extras/GradientControls'
import { TextWarpControls } from '@/components/warp/TextWarpControls'
import { call } from '@/lib/backend'
import { rgbaToHex } from '@/lib/color'
import { isTextLayer } from '@/lib/document'
import { pageKey, projectKey, refresh, usePage, useProject } from '@/lib/queries'
import { commands, type EntityId, type Layer, type Typography } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@koharu/ui/components/dialog'
import { Textarea } from '@koharu/ui/components/textarea'

import {
  EffectCheckbox,
  Field,
  OutlineControls,
  outlineEnabled,
  outerEnabled,
  toggleOutline,
} from './Controls'
import { useRedesignStore } from './state'
import { isMixed, useTypography } from './useTypography'

type TextLayer = Extract<Layer, { type: 'text' }>
type EffectSection = 'outline' | 'outer' | 'gradient' | 'warp'
type DetailSection = 'explain' | 'note' | 'why' | 'source' | 'split' | 'qc'

export function BoxDialogs() {
  const popup = useRedesignStore((state) => state.popup)
  const close = useRedesignStore((state) => state.close)
  const page = usePage().data
  const project = useProject().data?.name
  const key = JSON.stringify([project, page?.id])
  const previous = useRef(key)
  useEffect(() => {
    window.addEventListener(discussLineEvent, close)
    return () => window.removeEventListener(discussLineEvent, close)
  }, [close])
  useEffect(() => {
    if (previous.current !== key) close()
    previous.current = key
  }, [key, close])
  if (!popup || !page) return null
  const targets = popup.kind === 'fx' ? popup.targets : [popup.layer]
  const layers = page.layers.filter(isTextLayer).filter((layer) => targets.includes(layer.id))
  if (!layers.length) return null
  return popup.kind === 'fx' ? (
    <EffectsDialog key={JSON.stringify([key, targets])} targets={targets} />
  ) : (
    <DetailsDialog key={JSON.stringify([key, popup.layer])} layer={layers[0]} />
  )
}

function DialogFrame({
  title,
  description,
  children,
  footer,
}: {
  title: string
  description: string
  children: ReactNode
  footer?: ReactNode
}) {
  const { t } = useTranslation()
  const close = useRedesignStore((state) => state.close)
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) close()
      }}
    >
      <DialogContent
        showCloseButton={false}
        className='flex max-h-[85vh] w-[min(46rem,calc(100vw-2rem))] max-w-none flex-col gap-3 overflow-hidden sm:max-w-none'
      >
        <DialogTitle className='truncate pr-2 text-sm'>{title}</DialogTitle>
        <DialogDescription className='sr-only'>{description}</DialogDescription>
        {children}
        <footer className='flex shrink-0 items-center justify-end gap-2 border-t pt-3'>
          {footer}
          <Button size='sm' variant='outline' onClick={close}>
            {t('redesign.close')}
          </Button>
        </footer>
      </DialogContent>
    </Dialog>
  )
}

function SectionButton({
  active,
  children,
  onClick,
  disabled = false,
}: {
  active: boolean
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
}) {
  return (
    <button
      type='button'
      disabled={disabled}
      aria-pressed={active}
      className='w-full rounded-md px-2 py-2 text-left text-[11px] leading-4 hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-40 aria-pressed:bg-accent aria-pressed:text-accent-foreground'
      onClick={onClick}
    >
      {children}
    </button>
  )
}

function EffectsDialog({ targets }: { targets: EntityId[] }) {
  const { t } = useTranslation()
  const data = useTypography(targets)
  const [section, setSection] = useState<EffectSection>('outline')
  const [resetting, setResetting] = useState(false)
  const text =
    data.layers[0]?.content.translation?.text ||
    data.layers[0]?.content.source?.text ||
    t('redesign.emptyBox')
  const title =
    data.layers.length === 1
      ? t('redesign.effectsTitle', { text })
      : t('redesign.effectsMany', { count: data.layers.length })
  const effectItems = [
    {
      id: 'outline',
      label: t('redesign.outline'),
      read: outlineEnabled,
      update: (enabled: boolean) => data.apply((value) => toggleOutline(value, enabled)),
    },
    {
      id: 'outer',
      label: t('redesign.secondOutline'),
      read: outerEnabled,
      update: (enabled: boolean) => data.apply((value) => toggleOutline(value, enabled, true)),
    },
    {
      id: 'gradient',
      label: t('redesign.gradient'),
      read: (value: typeof data.typography) => !!value.fill_gradient,
      update: (enabled: boolean) =>
        data.apply((value) => ({ ...value, fill_gradient: enabled ? gradientFor(value) : null })),
    },
  ] as const
  return (
    <DialogFrame
      title={title}
      description={t('redesign.effectsDescription')}
      footer={
        <Button
          className='mr-auto'
          size='sm'
          variant='outline'
          disabled={resetting || data.disabled || data.pending}
          onClick={() => {
            setResetting(true)
            void call(commands.resetBoxEffects, targets)
              .then(() => refresh(projectKey, pageKey, ['box-effects']))
              .catch(() => undefined)
              .finally(() => setResetting(false))
          }}
        >
          {t('redesign.resetEffects')}
        </Button>
      }
    >
      <EffectsPreview typography={data.typography} text={text} />
      <div className='grid min-h-56 min-w-0 grid-cols-[minmax(8rem,0.85fr)_minmax(0,2fr)] gap-3 overflow-y-auto'>
        <nav
          className='grid content-start gap-1 border-r pr-3'
          aria-label={t('redesign.effectsSections')}
        >
          {effectItems.map((item) => (
            <div
              key={item.id}
              className='flex items-center gap-1 rounded-md px-1 aria-current:bg-accent'
              aria-current={section === item.id ? 'true' : undefined}
            >
              <EffectCheckbox
                hideLabel
                label={t('redesign.enableEffect', { effect: item.label })}
                checked={item.read(data.typography)}
                mixed={isMixed(data.values, item.read)}
                disabled={
                  resetting ||
                  data.disabled ||
                  (item.id === 'outer' && data.values.some((value) => !outlineEnabled(value)))
                }
                onChange={(enabled) => {
                  item.update(enabled)
                  setSection(item.id)
                }}
              />
              <SectionButton active={section === item.id} onClick={() => setSection(item.id)}>
                {item.label}
              </SectionButton>
            </div>
          ))}
          <SectionButton active={section === 'warp'} onClick={() => setSection('warp')}>
            {t('redesign.warp')}
          </SectionButton>
          <SectionButton active={false} disabled>
            {t('redesign.screentoneLater')}
          </SectionButton>
          <SectionButton active={false} disabled>
            {t('redesign.wavyLater')}
          </SectionButton>
        </nav>
        <div className='min-w-0 px-1 py-1'>
          {(section === 'outline' || section === 'outer') && (
            <OutlineControls {...data} disabled={resetting || data.disabled} section={section} />
          )}
          {section === 'gradient' && (
            <GradientControls {...data} disabled={resetting || data.disabled} />
          )}
          {section === 'warp' && <TextWarpControls layers={targets} />}
        </div>
      </div>
    </DialogFrame>
  )
}

function EffectsPreview({ typography, text }: { typography: Typography; text: string }) {
  const { t } = useTranslation()
  const gradientId = useId()
  const gradient = typography.fill_gradient
  const primary = outlineEnabled(typography) ? Math.min(typography.stroke_width ?? 0, 8) : 0
  const outer = outerEnabled(typography) ? Math.min(typography.outer_stroke_width ?? 0, 8) : 0
  const label = Array.from(text).slice(0, 80).join('')
  const properties = {
    x: 340,
    y: 45,
    dominantBaseline: 'middle' as const,
    textAnchor: 'middle' as const,
    fontSize: 32,
    fontFamily: typography.preferred_font ?? undefined,
    fontWeight: typography.font_weight ?? 400,
    fontStyle: typography.font_style ?? 'normal',
  }
  return (
    <svg
      role='img'
      aria-label={t('redesign.preview')}
      viewBox='0 0 680 88'
      className='h-24 w-full shrink-0 overflow-hidden rounded-md border bg-white'
    >
      {gradient && (
        <defs>
          {gradient.kind === 'Linear' ? (
            <linearGradient
              id={gradientId}
              gradientTransform={`rotate(${gradient.angle_degrees} .5 .5)`}
              x1='0'
              y1='.5'
              x2='1'
              y2='.5'
            >
              {gradient.stops.map((stop, index) => (
                <stop key={index} offset={stop.offset} stopColor={rgbaToHex(stop.color)} />
              ))}
            </linearGradient>
          ) : (
            <radialGradient id={gradientId}>
              {gradient.stops.map((stop, index) => (
                <stop key={index} offset={stop.offset} stopColor={rgbaToHex(stop.color)} />
              ))}
            </radialGradient>
          )}
        </defs>
      )}
      {outer > 0 && (
        <text
          {...properties}
          fill='none'
          stroke={rgbaToHex(typography.outer_stroke_color ?? [0, 0, 0, 255])}
          strokeWidth={(primary + outer) * 2}
          strokeLinejoin={typography.stroke_join ?? 'round'}
        >
          {label}
        </text>
      )}
      {primary > 0 && (
        <text
          {...properties}
          fill='none'
          stroke={rgbaToHex(typography.stroke_color ?? [255, 255, 255, 255])}
          strokeWidth={primary * 2}
          strokeLinejoin={typography.stroke_join ?? 'round'}
        >
          {label}
        </text>
      )}
      <text
        {...properties}
        fill={gradient ? `url(#${gradientId})` : rgbaToHex(typography.color ?? [0, 0, 0, 255])}
      >
        {label}
      </text>
    </svg>
  )
}

function DetailsDialog({ layer }: { layer: TextLayer }) {
  const { t } = useTranslation()
  const page = usePage().data
  const issues = useBoxIssues(page?.id ?? null)
  const [section, setSection] = useState<DetailSection>(() =>
    (issues.get(layer.id) ?? 0) > 0 ? 'qc' : 'explain',
  )
  const text =
    layer.content.translation?.text || layer.content.source?.text || t('redesign.emptyBox')
  return (
    <DialogFrame
      title={t('redesign.detailsTitle', { text })}
      description={t('redesign.detailsDescription')}
    >
      <div className='grid min-h-64 min-w-0 grid-cols-[minmax(8rem,0.85fr)_minmax(0,2fr)] gap-3 overflow-y-auto'>
        <nav
          className='grid content-start gap-1 border-r pr-3'
          aria-label={t('redesign.detailsSections')}
        >
          {(['explain', 'note', 'why', 'source', 'split', 'qc'] as const).map((value) => (
            <SectionButton key={value} active={section === value} onClick={() => setSection(value)}>
              {t(`redesign.details.${value}`)}
            </SectionButton>
          ))}
        </nav>
        <div className='grid min-w-0 content-start gap-3 px-1 py-1'>
          {section === 'explain' && <LineContextCard layer={layer.id} />}
          {section === 'note' && <BoxNoteLine layer={layer.id} expanded />}
          {section === 'why' && (
            <>
              <BubbleShapeLine layer={layer.id} />
              <LetteringCategoryLine layer={layer.id} editable={false} />
            </>
          )}
          {section === 'source' && <SourceSection layer={layer} />}
          {section === 'split' && <BoxActions layer={layer.id} />}
          {section === 'qc' && <BoxQcSection layer={layer.id} />}
        </div>
      </div>
    </DialogFrame>
  )
}

function SourceSection({ layer }: { layer: TextLayer }) {
  const { t } = useTranslation()
  const source = layer.content.source?.text ?? ''
  const translation = layer.content.translation?.text ?? ''
  return (
    <>
      <Field label={t('inspector.source')}>
        <GlossaryTextMenu field='source' source={source} translation={translation}>
          <Textarea
            data-testid={`edit-source-${layer.id}`}
            aria-label={t('redesign.sourceText')}
            readOnly
            value={source}
            className='min-h-20 w-full resize-y rounded-md border bg-muted/30 p-2 text-xs'
          />
        </GlossaryTextMenu>
      </Field>
      <Field label={t('inspector.translation')}>
        <GlossaryTextMenu field='translation' source={source} translation={translation}>
          <CommitTextarea
            data-testid={`edit-translation-${layer.id}`}
            aria-label={t('redesign.translationText')}
            value={translation}
            className='min-h-24 w-full resize-y text-xs'
            onCommit={(text) =>
              void call(commands.setTranslation, layer.id, text.trim() ? text : null)
                .then(() => refresh(projectKey, pageKey))
                .catch(() => undefined)
            }
          />
        </GlossaryTextMenu>
      </Field>
      <BoxRedoSection layers={[layer.id]} />
    </>
  )
}
