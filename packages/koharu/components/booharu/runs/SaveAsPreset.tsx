'use client'

import { BookmarkPlus } from 'lucide-react'
import { useId, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { NameError } from '@/components/booharu/runs/NameError'
import {
  addPreset,
  MAX_PRESETS,
  presetNameError,
  saveRuns,
  useRunPresets,
} from '@/lib/booharu/runs'
import { useKoharuStore } from '@/lib/store'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'

/** Save as preset, under the Run menu's stage switches: it asks for a name in place, then saves
 * the stages switched on as a preset, after the others, refusing a blank name, or one another
 * preset has, with a message. Escape, or Cancel, puts the button back. Once there are 100 presets,
 * it says so instead. `onSaved` runs once the preset is saved; a save that fails shows its error
 * as every failed command's does, and keeps the name for another try. */
export function SaveAsPreset({ onSaved }: { onSaved: () => void }) {
  const { t } = useTranslation()
  const presets = useRunPresets()
  const stages = useKoharuStore((state) => state.processingStages)
  const [naming, setNaming] = useState(false)
  const [name, setName] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const errorId = useId()
  if (!presets) return null
  const full = presets.length >= MAX_PRESETS

  const cancel = () => {
    setNaming(false)
    setName('')
    setError(null)
  }

  const save = () => {
    if (saving) return
    const refused = presetNameError(name, presets, t)
    if (refused) {
      setError(refused)
      return
    }
    setSaving(true)
    void saveRuns(addPreset(name, stages))
      .then(onSaved, () => undefined)
      .finally(() => setSaving(false))
  }

  if (!naming) {
    return (
      <div className='mt-1 grid gap-1 border-t border-border/60 pt-1'>
        <Button
          type='button'
          variant='ghost'
          size='sm'
          disabled={full}
          className='h-8 justify-start gap-2 rounded-lg px-2 text-[11px] font-normal text-muted-foreground hover:bg-primary/10 hover:text-foreground'
          onClick={() => setNaming(true)}
        >
          <BookmarkPlus className='size-3.5' /> {t('booharu.runs.presets.saveAs')}
        </Button>
        {full && (
          <p className='px-2 pb-1 text-[9px] text-muted-foreground'>
            {t('booharu.runs.presets.limit', { max: MAX_PRESETS })}
          </p>
        )}
      </div>
    )
  }

  return (
    <div className='mt-1 grid gap-1.5 border-t border-border/60 px-1 pt-2 pb-1'>
      <Input
        autoFocus
        value={name}
        maxLength={256}
        disabled={saving}
        placeholder={t('booharu.runs.presets.name')}
        aria-label={t('booharu.runs.presets.name')}
        aria-invalid={error !== null || undefined}
        aria-describedby={error ? errorId : undefined}
        className='h-7 text-[11px]'
        onChange={(event) => {
          setName(event.currentTarget.value)
          setError(null)
        }}
        onKeyDown={(event) => {
          // Enter that ends an input method's composition, as in Japanese, saves nothing.
          if (event.key === 'Enter' && !event.nativeEvent.isComposing) save()
          if (event.key === 'Escape') {
            // Escape puts the button back, and leaves the Run menu open.
            event.stopPropagation()
            cancel()
          }
        }}
      />
      {error && <NameError id={errorId}>{error}</NameError>}
      <div className='flex justify-end gap-1'>
        <Button type='button' variant='ghost' size='xs' className='text-[10px]' onClick={cancel}>
          {t('common.cancel')}
        </Button>
        <Button
          type='button'
          size='xs'
          className='text-[10px]'
          disabled={saving}
          aria-busy={saving}
          onClick={save}
        >
          {t('booharu.runs.presets.save')}
        </Button>
      </div>
    </div>
  )
}
