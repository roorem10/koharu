'use client'

import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ModelPicker } from '@/components/controls/ModelPicker'
import { canRead, listedModel, saveReading, useReadingSettings } from '@/lib/booharu/reading'
import { vendorOf } from '@/lib/models'
import { useModels } from '@/lib/queries'
import type { Model } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'

/** The models a chapter can be read with, of `models`: those that see images and call tools, with
 * `saved`, the Read chapter model, first when the list doesn't have it, as Koharu's own menu shows
 * the model in use. */
export function readingChoices(models: readonly Model[], saved: string | null): Model[] {
  const able = models.filter(canRead)
  if (!saved || listedModel(models, saved)) return able
  return [
    { reference: saved, name: saved, vendor: vendorOf(saved), images: true, tools: true },
    ...able,
  ]
}

/** The reading-model picker: Koharu's model menu, given only the models that can read a chapter's
 * pages, listed again as it opens, as Koharu's own menu is, with the saved Read chapter model
 * checked. A pick saves it as `[booharu-reading] model` at once, the menu taking no other pick
 * till it's saved, then `onPicked` runs with it; a save that fails shows its error as every failed
 * command's does, and picks nothing. `onCancel` gives it a heading with Cancel, which picks
 * nothing, for a menu it opens inside of; a dialog has its own. Under the menu, what to pick and
 * that the pick is saved, unless `hint` is off, for a dialog that says it above. */
export function ReadingModelPicker({
  onPicked,
  onCancel,
  hint = true,
}: {
  onPicked: (reference: string) => void
  onCancel?: () => void
  hint?: boolean
}) {
  const { t } = useTranslation()
  const models = useModels()
  const { refetch } = models
  const saved = useReadingSettings()?.model ?? null
  const [saving, setSaving] = useState<string | null>(null)
  const mounted = useRef(true)

  // The models are listed again as the menu opens; a listing that fails shows its error, as every
  // failed read's does.
  useEffect(() => {
    mounted.current = true
    void refetch()
    return () => {
      mounted.current = false
    }
  }, [refetch])

  const pick = (reference: string) => {
    if (saving) return
    setSaving(reference)
    void saveReading({ model: reference })
      .then(
        () => onPicked(reference),
        () => undefined,
      )
      .finally(() => {
        if (mounted.current) setSaving(null)
      })
  }

  return (
    <div className='grid min-w-0 gap-1'>
      {onCancel && (
        <div className='flex h-7 min-w-0 items-center gap-2 border-b border-border/60 pr-0.5 pb-1 pl-2'>
          <span className='min-w-0 flex-1 truncate text-[11px] font-medium'>
            {t('booharu.context.readingModel')}
          </span>
          <Button
            type='button'
            variant='ghost'
            size='xs'
            className='text-[10px] text-muted-foreground'
            disabled={saving !== null}
            onClick={onCancel}
          >
            {t('common.cancel')}
          </Button>
        </div>
      )}
      <ModelPicker
        value={saved}
        models={readingChoices(models.data ?? [], saved)}
        loading={models.isFetching}
        disabled={saving !== null}
        onSelect={pick}
      />
      {hint && (
        <p className='px-2 pt-0.5 pb-1 text-[10px] leading-snug text-muted-foreground'>
          {t('booharu.context.pickReader')} {t('booharu.context.readerDownload')}
        </p>
      )}
    </div>
  )
}
