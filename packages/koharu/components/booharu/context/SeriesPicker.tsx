'use client'

import { Plus } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { createSeries, useSeriesList } from '@/lib/booharu/context'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

/** The series picker, as Booharu 0.0.4 shipped it: every series by name, `value` chosen, and its
 * own New series… button beside it, which opens a name field under it. Picking a series, or
 * creating one, calls `onChange` with its id: the Context tab links the chapter to it, the
 * glossary editor shows it. A failed creation shows its error, as every failed command's does,
 * and keeps the name typed. */
export function SeriesPicker({
  value,
  onChange,
  disabled,
}: {
  value: string | null
  onChange: (id: string) => Promise<unknown>
  disabled?: boolean
}) {
  const { t } = useTranslation()
  const list = useSeriesList().data ?? []
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const chosen = list.find((series) => series.id === value) ?? null

  const create = () => {
    setBusy(true)
    void createSeries(name)
      .then(async (series) => {
        setCreating(false)
        setName('')
        await onChange(series.id)
      })
      .catch(() => undefined)
      .finally(() => setBusy(false))
  }

  return (
    <div className='grid min-w-0 gap-1.5'>
      <div className='flex min-w-0 items-center gap-1'>
        <Select
          value={chosen?.id ?? null}
          items={Object.fromEntries(list.map((series) => [series.id, series.name]))}
          disabled={disabled}
          onValueChange={(id) => {
            if (id && id !== chosen?.id) void onChange(id).catch(() => undefined)
          }}
        >
          <SelectTrigger
            aria-label={t('booharu.context.series')}
            title={chosen?.name}
            className='h-7 min-w-0 flex-1 text-[11px]'
          >
            <SelectValue placeholder={t('booharu.context.pickSeries')} className='truncate' />
          </SelectTrigger>
          <SelectContent>
            {list.map((series) => (
              <SelectItem key={series.id} value={series.id}>
                {series.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type='button'
          size='sm'
          variant='outline'
          aria-expanded={creating}
          disabled={disabled}
          className='h-7 text-[11px]'
          onClick={() => setCreating((open) => !open)}
        >
          <Plus /> {t('booharu.context.newSeries')}
        </Button>
      </div>
      {creating && (
        <form
          className='flex min-w-0 items-center gap-1'
          onSubmit={(event) => {
            event.preventDefault()
            if (name.trim() && !busy) create()
          }}
        >
          <Input
            autoFocus
            aria-label={t('booharu.context.seriesName')}
            placeholder={t('booharu.context.seriesName')}
            value={name}
            disabled={busy}
            className='h-7 text-[11px]'
            onChange={(event) => setName(event.currentTarget.value)}
            onKeyDown={(event) => {
              if (event.key !== 'Escape') return
              // Escape closes the name field, not a dialog it's in.
              event.stopPropagation()
              setCreating(false)
            }}
          />
          <Button
            type='submit'
            size='sm'
            disabled={busy || !name.trim()}
            aria-busy={busy}
            className='h-7 text-[11px]'
          >
            {t('booharu.context.create')}
          </Button>
        </form>
      )}
    </div>
  )
}
