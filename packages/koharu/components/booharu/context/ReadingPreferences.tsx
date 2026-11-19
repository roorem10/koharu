'use client'

import { Check, ChevronDown, Minus, Plus } from 'lucide-react'
import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { readingChoices } from '@/components/booharu/context/ReadingModelPicker'
import { ModelPicker } from '@/components/controls/ModelPicker'
import { vendorName } from '@/components/preferences/models'
import { PreferenceRow, PreferenceSection } from '@/components/preferences/PreferenceFields'
import { useBooharuPreferences } from '@/lib/booharu/preferences'
import {
  cantRead,
  completeReading,
  isPagesPerRequest,
  MAX_PAGES_PER_REQUEST,
  saveReading,
  type Reading,
} from '@/lib/booharu/reading'
import { useKoharuStore } from '@/lib/store'
import { modelName, vendorOf } from '@/lib/translation'
import type { CantRead } from '@koharu/ipc/protocol'
import { Badge } from '@koharu/ui/components/badge'
import { Button } from '@koharu/ui/components/button'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'
import { Popover, PopoverContent, PopoverTrigger } from '@koharu/ui/components/popover'
import { Switch } from '@koharu/ui/components/switch'

/** The string key that says why a model can't read a chapter's pages. */
const why = (reason: CantRead) =>
  reason === 'no_vision' ? 'booharu.context.noVision' : 'booharu.context.onlyTranslates'

/** Read chapter's section of Settings > Pipeline, after the translator's, the `[booharu-reading]`
 * table: the model that reads, of those that can, or the translation model; whether the Context
 * tab asks before it reads; and how many pages one request sends. Each change saves the section at
 * once, over what the file holds just before, as the other Booharu settings do; once the last save
 * ends, what the file holds shows, so after a failed save, whose error shows as every failed
 * command's does, the file's values show. */
export function ReadingPreferences() {
  const { t } = useTranslation()
  const preferences = useBooharuPreferences()
  const [unsaved, setUnsaved] = useState<Reading | null>(null)
  /** Counts changes, so a save that ends knows whether a later change is still being saved. */
  const changes = useRef(0)
  const saved = preferences.data ? completeReading(preferences.data['booharu-reading']) : undefined
  const reading = unsaved ?? saved

  const change = (values: Partial<Reading>) => {
    if (!reading) return
    changes.current += 1
    const last = changes.current
    setUnsaved({ ...reading, ...values })
    void saveReading(values)
      .catch(() => undefined)
      .finally(() => {
        if (changes.current === last) setUnsaved(null)
      })
  }

  return (
    <PreferenceSection
      title={t('booharu.context.readChapter')}
      description={t('booharu.context.readingDescription')}
    >
      <PreferenceRow
        title={t('booharu.context.readingModel')}
        description={t('booharu.context.readingModelDescription')}
        align='start'
      >
        <ReadingModel
          value={reading?.model ?? null}
          disabled={!reading}
          onChange={(model) => change({ model })}
        />
      </PreferenceRow>
      <PreferenceRow
        title={t('booharu.context.askBeforeReading')}
        description={t('booharu.context.askBeforeReadingDescription')}
      >
        <Switch
          aria-label={t('booharu.context.askBeforeReading')}
          checked={reading?.ask ?? true}
          disabled={!reading}
          onCheckedChange={(ask) => change({ ask })}
        />
      </PreferenceRow>
      <PreferenceRow
        title={t('booharu.context.pagesPerRequest')}
        description={t('booharu.context.pagesPerRequestDescription', {
          max: MAX_PAGES_PER_REQUEST,
        })}
      >
        <NumberField
          className='w-28'
          value={reading?.pages_per_request ?? null}
          min={1}
          max={MAX_PAGES_PER_REQUEST}
          step={1}
          disabled={!reading}
          onValueChange={(pages) => {
            // A cleared or half-typed field saves nothing, and shows the pages again when it loses
            // focus; the field keeps a typed number within 1 to 40.
            if (!isPagesPerRequest(pages) || pages === reading?.pages_per_request) return
            change({ pages_per_request: pages })
          }}
        >
          <NumberFieldGroup>
            <NumberFieldDecrement aria-label={t('booharu.context.fewerPages')}>
              <Minus />
            </NumberFieldDecrement>
            <NumberFieldInput aria-label={t('booharu.context.pagesPerRequest')} />
            <NumberFieldIncrement aria-label={t('booharu.context.morePages')}>
              <Plus />
            </NumberFieldIncrement>
          </NumberFieldGroup>
        </NumberField>
      </PreferenceRow>
    </PreferenceSection>
  )
}

/** The Read chapter model, `value`, or the translation model for none, chosen from a menu laid out
 * as the translator's: Same as translation model, then Koharu's model menu given only the models
 * that can read a chapter's pages. Under it, why the model a read would use can't read them, when
 * it can't. */
function ReadingModel({
  value,
  disabled,
  onChange,
}: {
  value: string | null
  disabled: boolean
  onChange: (model: string | null) => void
}) {
  const { t } = useTranslation()
  const [open, setOpen] = useState(false)
  const models = useKoharuStore((state) => state.models)
  const translation = useKoharuStore(
    (state) => state.preferences?.pipeline.translator?.model ?? null,
  )
  const choose = (model: string | null) => {
    setOpen(false)
    if (model !== value) onChange(model)
  }
  // The saved model, else the translation model, which a read refuses with when it can't read.
  const reader = value ?? translation
  const reason = reader ? cantRead(models, reader) : null

  return (
    <div className='grid min-w-0 gap-1.5'>
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger
          type='button'
          aria-label={t('booharu.context.readingModel')}
          disabled={disabled}
          className='flex h-8 w-full min-w-0 items-center justify-between gap-2 rounded-lg border border-input bg-transparent px-2.5 text-[11px] transition-colors outline-none hover:bg-foreground/[0.03] focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50 disabled:opacity-50'
        >
          <span className='flex min-w-0 flex-1 items-center gap-2 text-left'>
            {value && (
              <Badge variant='outline' className='shrink-0 px-1.5 py-0 text-[9px] font-medium'>
                {vendorName(vendorOf(value), t)}
              </Badge>
            )}
            <span className='truncate'>
              {value ? modelName(models, value) : t('booharu.context.sameModel')}
            </span>
          </span>
          <ChevronDown className='size-3.5 shrink-0 text-muted-foreground' />
        </PopoverTrigger>
        <PopoverContent
          align='start'
          sideOffset={4}
          className='w-(--anchor-width) min-w-64 gap-0 overflow-hidden rounded-xl border border-border/50 p-1 shadow-sm ring-0'
        >
          <Button
            type='button'
            variant='ghost'
            size='sm'
            aria-pressed={value === null}
            className='mb-0.5 h-8 justify-start gap-2 rounded-lg border-b border-border/60 px-2 text-[11px] font-normal hover:bg-primary/10'
            onClick={() => choose(null)}
          >
            <span className='min-w-0 flex-1 truncate text-left'>
              {t('booharu.context.sameModel')}
            </span>
            {value === null && <Check className='size-3.5 shrink-0' />}
          </Button>
          <ModelPicker
            value={value}
            models={readingChoices(models, value)}
            onSelect={(reference) => choose(reference)}
          />
        </PopoverContent>
      </Popover>
      {reader && reason && (
        <p role='status' className='text-[10px] leading-4 text-destructive'>
          {t(why(reason), { model: modelName(models, reader) })}
        </p>
      )}
    </div>
  )
}
