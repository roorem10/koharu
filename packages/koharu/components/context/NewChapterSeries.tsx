'use client'

import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { commands } from '@koharu/bridge/protocol'
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
  return (
    <div className='grid min-w-0 gap-1.5'>
      <span className='text-xs font-medium'>{title}</span>
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
        <SelectTrigger className='w-full min-w-0' aria-label={title}>
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
    </div>
  )
}
