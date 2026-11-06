'use client'

import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Input } from '@koharu/ui/components/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

import { seriesKey, useSeries } from './data'
import type { Series } from './types'

export function useNewChapterSeries(enabled = true) {
  const series = useSeries()
  const last = useQuery({
    queryKey: [...seriesKey, 'last-used'],
    queryFn: async () => (await call(commands.getLastSeries)) as Series | null,
    enabled,
  })
  const [selection, setSelection] = useState<string | null | undefined>(undefined)
  const suggested = selection === undefined ? (last.data?.id ?? null) : selection
  const options = series.data ?? []
  const value = options.some((item) => item.id === suggested) ? suggested : null
  return {
    options,
    value,
    setValue: setSelection,
    loading: series.isPending || last.isPending,
  }
}

export function NewChapterSeries({
  choice,
  disabled,
  label,
}: {
  choice: ReturnType<typeof useNewChapterSeries>
  disabled?: boolean
  label?: string
}) {
  const { t } = useTranslation()
  const title = label ?? t('chapterContext.series')
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const create = () => {
    if (busy || !name.trim()) return
    setBusy(true)
    void call(commands.createSeries, name)
      .then(async (value) => {
        // The new series must be listed before it can be the selected option.
        await queryClient.invalidateQueries({ queryKey: seriesKey })
        choice.setValue((value as Series).id)
        setCreating(false)
        setName('')
      })
      .catch(() => undefined)
      .finally(() => setBusy(false))
  }
  return (
    <div className='grid min-w-0 gap-1.5'>
      <span className='text-xs font-medium'>{title}</span>
      <div className='flex min-w-0 items-center gap-1'>
        <Select
          value={choice.value ?? ''}
          items={Object.fromEntries([
            ['', t('chapterContext.noSeries')],
            ...choice.options.map((item) => [item.id, item.name]),
          ])}
          disabled={disabled || choice.loading}
          onValueChange={(id) => {
            if (id !== null) choice.setValue(id || null)
          }}
        >
          <SelectTrigger className='min-w-0 flex-1' aria-label={title}>
            <SelectValue className='truncate' />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value=''>{t('chapterContext.noSeries')}</SelectItem>
            {choice.options.map((item) => (
              <SelectItem key={item.id} value={item.id}>
                {item.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          type='button'
          size='sm'
          variant='outline'
          disabled={disabled}
          onClick={() => setCreating(!creating)}
        >
          {t('chapterContext.newSeries')}
        </Button>
      </div>
      {creating && (
        // Not a form: this control sits inside other dialogs' and pages' layouts.
        <div className='flex gap-1'>
          <Input
            autoFocus
            aria-label={t('chapterContext.seriesName')}
            value={name}
            disabled={disabled}
            onChange={(event) => setName(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                create()
              }
            }}
          />
          <Button
            type='button'
            size='sm'
            disabled={disabled || busy || !name.trim()}
            onClick={create}
          >
            {t('chapterContext.create')}
          </Button>
        </div>
      )}
    </div>
  )
}
