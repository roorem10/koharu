'use client'

import { Minus, Plus, Trash2 } from 'lucide-react'
import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { NameError } from '@/components/booharu/runs/NameError'
import {
  PreferencePage,
  PreferenceRow,
  PreferenceSection,
} from '@/components/preferences/PreferenceFields'
import {
  addPreset,
  deletePreset,
  isRetries,
  MAX_PRESETS,
  MAX_RETRIES,
  presetLabel,
  presetNameError,
  renamePreset,
  runStages,
  setRuns,
  tickPresetStage,
  useRunsSettings,
  type Runs,
  type RunsEdit,
} from '@/lib/booharu/runs'
import { pipelineStages } from '@/lib/store'
import type { OnPageFailure, RunOrder, RunPreset } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Input } from '@koharu/ui/components/input'
import {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
} from '@koharu/ui/components/number-field'
import { RadioGroup, RadioGroupItem } from '@koharu/ui/components/radio-group'
import { Switch } from '@koharu/ui/components/switch'

/** A preset's row, laid out as Koharu's preference rows are. */
const ROW = 'grid grid-cols-[minmax(150px,0.9fr)_minmax(220px,1.1fr)] items-start gap-6 py-3.5'

/** The longest preset name the fields take. */
const NAME_LENGTH = 256

/** Settings > Runs, the `[booharu-runs]` table: the Run menu's presets, which switch on their
 * stages and start nothing, then how every run goes when one of its pages fails, the order its
 * stages go in, and whether a run over pages chimes as it ends. A change saves at once, over what
 * the file holds just before, as the other Booharu settings do. */
export function RunsPreferences() {
  const { t } = useTranslation()
  const { runs, failed, change } = useRunsSettings()

  return (
    <PreferencePage title={t('booharu.runs.title')} description={t('booharu.runs.description')}>
      {runs ? (
        <>
          <Presets presets={runs.presets} change={change} />
          <Behavior runs={runs} change={change} />
        </>
      ) : (
        <p role='status' className='text-[12px] text-muted-foreground'>
          {t(failed ? 'booharu.runs.loadError' : 'booharu.runs.loading')}
        </p>
      )}
    </PreferencePage>
  )
}

/** The presets, each with its name, renamed in place, its stages, and Delete, then the row that
 * adds one. */
function Presets({ presets, change }: { presets: RunPreset[]; change: (edit: RunsEdit) => void }) {
  const { t } = useTranslation()
  return (
    <PreferenceSection
      title={t('booharu.runs.presets.title')}
      description={t('booharu.runs.presets.description')}
    >
      {presets.length === 0 && (
        <p className='py-3.5 text-[11px] text-muted-foreground'>
          {t('booharu.runs.presets.empty')}
        </p>
      )}
      {presets.map((preset) => (
        // A preset's name is its key, as no two share one: a renamed preset's row starts over.
        <PresetRow key={preset.name} preset={preset} presets={presets} change={change} />
      ))}
      <NewPreset presets={presets} change={change} />
    </PreferenceSection>
  )
}

/** A preset's row, named after the name it shows: its name, its four stages with Read chapter
 * before Translation, which it takes only together with Translation, and Delete. */
function PresetRow({
  preset,
  presets,
  change,
}: {
  preset: RunPreset
  presets: RunPreset[]
  change: (edit: RunsEdit) => void
}) {
  const { t } = useTranslation()
  const label = presetLabel(preset, t)
  return (
    <div role='group' aria-label={label} className={ROW}>
      <PresetName
        name={preset.name}
        label={label}
        presets={presets}
        onRename={(name) => change(renamePreset(preset.name, name))}
      />
      <div className='flex min-w-0 items-start gap-2'>
        <div className='grid min-w-0 flex-1 grid-cols-2 gap-x-4 gap-y-2 pt-1.5'>
          {runStages.map((stage) => {
            const on = preset.stages.includes(stage)
            const others = preset.stages.filter((candidate) => candidate !== 'read')
            return (
              <label key={stage} className='flex min-w-0 items-center gap-2 text-[11px]'>
                <Checkbox
                  checked={on}
                  // A preset keeps at least one stage other than Read chapter, which goes only
                  // together with Translation.
                  disabled={
                    stage === 'read'
                      ? !preset.stages.includes('translate')
                      : on && others.length === 1
                  }
                  onCheckedChange={(next) => change(tickPresetStage(preset.name, stage, next))}
                />
                <span className='truncate'>
                  {stage === 'read' ? t('booharu.runs.read.label') : t(`phase.${stage}`)}
                </span>
              </label>
            )
          })}
        </div>
        <Button
          type='button'
          size='icon-sm'
          variant='ghost'
          aria-label={t('booharu.runs.presets.delete')}
          title={t('booharu.runs.presets.delete')}
          className='text-muted-foreground hover:text-destructive'
          onClick={() => change(deletePreset(preset.name))}
        >
          <Trash2 />
        </Button>
      </div>
    </div>
  )
}

/** A preset's name, `label` as it shows, renamed in place: Enter, or leaving the field, saves the
 * name typed, and Escape puts the saved one back. A blank name, or one another preset has, is
 * refused with a message, and stays in the field until it's changed or put back. */
function PresetName({
  name,
  label,
  presets,
  onRename,
}: {
  name: string
  label: string
  presets: RunPreset[]
  onRename: (name: string) => void
}) {
  const { t } = useTranslation()
  const [draft, setDraft] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const errorId = useId()

  const commit = () => {
    if (draft === null) return
    if (draft.trim() === label) {
      setDraft(null)
      setError(null)
      return
    }
    const refused = presetNameError(draft, presets, t, name)
    if (refused) {
      setError(refused)
      return
    }
    onRename(draft)
    setDraft(null)
  }

  return (
    <div className='grid min-w-0 gap-1'>
      <Input
        value={draft ?? label}
        maxLength={NAME_LENGTH}
        aria-label={t('booharu.runs.presets.name')}
        aria-invalid={error !== null || undefined}
        aria-describedby={error ? errorId : undefined}
        className='h-8 text-[12px]'
        onChange={(event) => {
          setDraft(event.currentTarget.value)
          setError(null)
        }}
        onBlur={commit}
        onKeyDown={(event) => {
          // Enter that ends an input method's composition, as in Japanese, commits nothing.
          if (event.key === 'Enter' && !event.nativeEvent.isComposing) event.currentTarget.blur()
          if (event.key === 'Escape') {
            setDraft(null)
            setError(null)
          }
        }}
      />
      {error && <NameError id={errorId}>{error}</NameError>}
    </div>
  )
}

/** The row that adds a preset with every stage: its name typed in, saved by Add or Enter, or
 * refused with a message when it's blank or another preset has it. Once there are 100 presets, it
 * says so instead. */
function NewPreset({
  presets,
  change,
}: {
  presets: RunPreset[]
  change: (edit: RunsEdit) => void
}) {
  const { t } = useTranslation()
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const errorId = useId()
  const full = presets.length >= MAX_PRESETS

  const add = () => {
    const refused = presetNameError(name, presets, t)
    if (refused) {
      setError(refused)
      return
    }
    change(addPreset(name, pipelineStages))
    setName('')
  }

  return (
    <div className={ROW}>
      <div className='grid min-w-0 gap-1'>
        <Input
          value={name}
          maxLength={NAME_LENGTH}
          disabled={full}
          placeholder={t('booharu.runs.presets.newName')}
          aria-label={t('booharu.runs.presets.newName')}
          aria-invalid={error !== null || undefined}
          aria-describedby={error ? errorId : undefined}
          className='h-8 text-[12px]'
          onChange={(event) => {
            setName(event.currentTarget.value)
            setError(null)
          }}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && !event.nativeEvent.isComposing) add()
          }}
        />
        {error && <NameError id={errorId}>{error}</NameError>}
      </div>
      <div className='flex min-w-0 items-center gap-3'>
        <Button type='button' size='sm' variant='outline' disabled={full} onClick={add}>
          <Plus /> {t('booharu.runs.presets.add')}
        </Button>
        {full && (
          <p className='text-[10px] leading-4 text-muted-foreground'>
            {t('booharu.runs.presets.limit', { max: MAX_PRESETS })}
          </p>
        )}
      </div>
    </div>
  )
}

/** How every run goes: the retries of a failed page stage, what happens once it has failed every
 * try, the order the stages go in, and the finish sound. */
function Behavior({ runs, change }: { runs: Runs; change: (edit: RunsEdit) => void }) {
  const { t } = useTranslation()
  return (
    <PreferenceSection
      title={t('booharu.runs.behavior.title')}
      description={t('booharu.runs.behavior.description')}
    >
      <PreferenceRow
        title={t('booharu.runs.retries')}
        description={t('booharu.runs.retriesDescription')}
      >
        <NumberField
          className='w-28'
          value={runs.retries}
          min={0}
          max={MAX_RETRIES}
          step={1}
          onValueChange={(retries) => {
            // A cleared or half-typed field saves nothing, and shows the retries again when it
            // loses focus; the field keeps a typed number within 0 to 5.
            if (!isRetries(retries) || retries === runs.retries) return
            change(setRuns({ retries }))
          }}
        >
          <NumberFieldGroup>
            <NumberFieldDecrement aria-label={t('booharu.runs.fewerRetries')}>
              <Minus />
            </NumberFieldDecrement>
            <NumberFieldInput aria-label={t('booharu.runs.retries')} />
            <NumberFieldIncrement aria-label={t('booharu.runs.moreRetries')}>
              <Plus />
            </NumberFieldIncrement>
          </NumberFieldGroup>
        </NumberField>
      </PreferenceRow>
      <PreferenceRow title={t('booharu.runs.onFailure')} align='start'>
        <Choice<OnPageFailure>
          label={t('booharu.runs.onFailure')}
          value={runs.on_page_failure}
          options={[
            { value: 'continue', label: t('booharu.runs.onFailureModes.continue') },
            { value: 'stop', label: t('booharu.runs.onFailureModes.stop') },
          ]}
          onChange={(on_page_failure) => change(setRuns({ on_page_failure }))}
        />
      </PreferenceRow>
      <PreferenceRow title={t('booharu.runs.order')} align='start'>
        <Choice<RunOrder>
          label={t('booharu.runs.order')}
          value={runs.order}
          options={[
            { value: 'pages', label: t('booharu.runs.orders.pages') },
            {
              value: 'stages',
              label: t('booharu.runs.orders.stages'),
              hint: t('booharu.runs.orders.stagesHint'),
            },
          ]}
          onChange={(order) => change(setRuns({ order }))}
        />
      </PreferenceRow>
      <PreferenceRow
        title={t('booharu.runs.finishSound')}
        description={t('booharu.runs.finishSoundDescription')}
      >
        <Switch
          aria-label={t('booharu.runs.finishSound')}
          checked={runs.finish_sound}
          onCheckedChange={(finish_sound) => change(setRuns({ finish_sound }))}
        />
      </PreferenceRow>
    </PreferenceSection>
  )
}

/** One of a few choices, each a radio button with its label, and its hint under it, if it has
 * one, which the button names as its description. */
function Choice<Value extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string
  value: Value
  options: ReadonlyArray<{ value: Value; label: string; hint?: string }>
  onChange: (value: Value) => void
}) {
  const id = useId()
  return (
    <RadioGroup
      aria-label={label}
      value={value}
      className='gap-2.5'
      onValueChange={(next) => next !== value && onChange(next as Value)}
    >
      {options.map((option) => (
        <div key={option.value} className='grid gap-0.5'>
          <label className='flex items-center gap-2 text-[11px]'>
            <RadioGroupItem
              value={option.value}
              aria-describedby={option.hint ? `${id}-${option.value}` : undefined}
            />
            {option.label}
          </label>
          {option.hint && (
            <p
              id={`${id}-${option.value}`}
              className='pl-6 text-[10px] leading-4 text-muted-foreground'
            >
              {option.hint}
            </p>
          )}
        </div>
      ))}
    </RadioGroup>
  )
}
