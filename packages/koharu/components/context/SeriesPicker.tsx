'use client'

import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
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

import { refreshContext, useSeries } from './data'
import type { Series } from './types'

export function SeriesPicker({
  value,
  onChange,
}: {
  value: string | null
  onChange: (id: string) => Promise<void>
}) {
  const { t } = useTranslation()
  const series = useSeries().data ?? []
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  return (
    <div className='grid min-w-0 gap-2'>
      <div className='flex min-w-0 items-center gap-1'>
        <Select
          value={value ?? ''}
          items={Object.fromEntries(series.map((item) => [item.id, item.name]))}
          onValueChange={(id) => {
            if (id) void onChange(id).catch(() => undefined)
          }}
        >
          <SelectTrigger
            className='min-w-0 flex-1'
            aria-label={t('chapterContext.series')}
            title={series.find((item) => item.id === value)?.name}
          >
            <SelectValue className='truncate' placeholder={t('chapterContext.pickSeries')} />
          </SelectTrigger>
          <SelectContent>
            {series.map((item) => (
              <SelectItem key={item.id} value={item.id}>
                {item.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button size='sm' variant='outline' onClick={() => setCreating(!creating)}>
          {t('chapterContext.newSeries')}
        </Button>
      </div>
      {creating && (
        <form
          className='flex gap-1'
          onSubmit={(event) => {
            event.preventDefault()
            setBusy(true)
            void call(commands.createSeries, name)
              .then(async (value) => {
                const item = value as Series
                await refreshContext()
                await onChange(item.id)
                setCreating(false)
                setName('')
              })
              .catch(() => undefined)
              .finally(() => setBusy(false))
          }}
        >
          <Input
            autoFocus
            aria-label={t('chapterContext.seriesName')}
            value={name}
            onChange={(event) => setName(event.target.value)}
          />
          <Button size='sm' type='submit' disabled={busy || !name.trim()}>
            {t('chapterContext.create')}
          </Button>
        </form>
      )}
    </div>
  )
}
