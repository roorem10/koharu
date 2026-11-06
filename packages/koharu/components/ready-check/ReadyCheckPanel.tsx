'use client'

import { useIsMutating, useMutation } from '@tanstack/react-query'
import { AlertTriangle, CheckCircle2, CircleAlert } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { useKoharuStore } from '@/lib/store'
import { commands, type ReadyData, type ReadyStatus } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Progress } from '@koharu/ui/components/progress'

import { refreshReadyCheck, useReadyCheck, useReadySettings } from './data'
import { FontSetup } from './FontSetup'
import { SeriesFix } from './SeriesFix'

const statusIcons = { ready: CheckCircle2, warning: AlertTriangle, missing: CircleAlert }

export function ReadyCheckPanel({ showDescription = true }: { showDescription?: boolean }) {
  const { t } = useTranslation()
  const check = useReadyCheck()
  const [fixing, setFixing] = useState<string | null>(null)
  const openSettings = useReadySettings((state) => state.open)
  return (
    <div className='grid min-w-0 gap-3'>
      <div className='flex items-center justify-between gap-3'>
        {showDescription && (
          <p className='text-xs text-muted-foreground'>{t('readyCheck.description')}</p>
        )}
        <Button
          size='sm'
          variant='outline'
          disabled={check.isFetching}
          onClick={() => void check.refetch()}
        >
          {t('readyCheck.refresh')}
        </Button>
      </div>
      {check.isPending && <p role='status'>{t('settings.loading')}</p>}
      {check.isError && (
        <p role='alert' className='text-sm text-destructive'>
          {t('readyCheck.loadError')}
        </p>
      )}
      {check.data?.map(({ status, reason, data }) => (
        <section
          key={data.kind}
          className='min-w-0 rounded-xl border border-border/80 bg-[var(--surface-panel)] p-4'
        >
          <div className='flex items-start gap-3'>
            <Status status={status} />
            <div className='min-w-0 flex-1'>
              <h3 className='text-sm font-medium'>{t(`readyCheck.items.${data.kind}`)}</h3>
              <p className='mt-1 text-xs text-muted-foreground'>
                {t(`readyCheck.reasons.${reason}`)}
              </p>
            </div>
            {data.kind !== 'models' &&
              data.kind !== 'export' &&
              !(data.kind === 'series' && !data.project_name) && (
                <Button
                  size='sm'
                  variant='outline'
                  aria-label={t('readyCheck.fixItem', { item: t(`readyCheck.items.${data.kind}`) })}
                  onClick={() => {
                    if (data.kind === 'translation') {
                      const providerConfigured =
                        reason === 'reading_model_unset'
                          ? data.reading_provider_configured
                          : data.provider_configured
                      openSettings(providerConfigured ? 'translation' : 'providers')
                    } else if (data.kind === 'target_language') openSettings('translation')
                    else setFixing((current) => (current === data.kind ? null : data.kind))
                  }}
                >
                  {t('readyCheck.fix')}
                </Button>
              )}
          </div>
          <div className='mt-3 min-w-0 text-xs'>
            <Details data={data} />
            {data.kind === 'series' && data.project_name && fixing === 'series' && (
              <div className='mt-3'>
                <SeriesFix
                  key={data.project_name}
                  projectName={data.project_name}
                  seriesId={data.series_name ? data.series_id : null}
                />
              </div>
            )}
            {data.kind === 'fonts' && fixing === 'fonts' && (
              <div className='mt-3 grid gap-3'>
                <FontSetup onSaved={refreshReadyCheck} />
                <Button size='sm' variant='outline' onClick={() => openSettings('typesetting')}>
                  {t('settings.tabs.typesetting')}
                </Button>
              </div>
            )}
          </div>
        </section>
      ))}
    </div>
  )
}

function Status({ status }: { status: ReadyStatus }) {
  const { t } = useTranslation()
  const Icon = statusIcons[status]
  return (
    <span
      className={
        status === 'ready'
          ? 'text-green-600 dark:text-green-400'
          : 'text-amber-600 dark:text-amber-400'
      }
    >
      <Icon aria-hidden='true' className='mt-0.5 size-4' />
      <span className='sr-only'>{t(`readyCheck.status.${status}`)}</span>
    </span>
  )
}

function Details({ data }: { data: ReadyData }) {
  const { t, i18n } = useTranslation()
  const languages = useKoharuStore((state) => state.preferences?.languages)
  switch (data.kind) {
    case 'translation':
      return (
        <div className='grid gap-1 break-words'>
          <p>
            {data.model.model || data.model.provider} ·{' '}
            {t(data.vision ? 'readyCheck.vision' : 'readyCheck.noVision')}
          </p>
          <p>
            {t('readyCheck.readingModel', {
              model: data.reading_model.model || data.reading_model.provider,
            })}{' '}
            · {t(data.reading_vision ? 'readyCheck.vision' : 'readyCheck.noVision')}
          </p>
          {!data.reading_enabled && <p>{t('readyCheck.readingOff')}</p>}
        </div>
      )
    case 'target_language': {
      const language = languages?.find((language) => language.tag === data.language)
      let label = language?.name || data.language
      if (data.language) {
        try {
          const locale = new Intl.Locale(data.language)
          const variants = languages?.filter(
            (candidate) => candidate.tag.split('-')[0] === locale.language,
          )
          const tag =
            variants && variants.length > 1
              ? locale.baseName
              : [locale.language, locale.script].filter(Boolean).join('-')
          label = new Intl.DisplayNames([i18n.language], { type: 'language' }).of(tag) || label
        } catch {
          /* Keep the native label for an unsupported language tag. */
        }
      }
      return <p>{label}</p>
    }
    case 'series':
      return data.series_name && <p className='break-words'>{data.series_name}</p>
    case 'fonts':
      return (
        <ul className='grid gap-1.5 sm:grid-cols-2'>
          {data.categories.map((font) => (
            <li key={font.category} className='min-w-0 break-words'>
              <span className='font-medium'>{t(`lettering.categories.${font.category}`)}</span>:{' '}
              {font.font || t('readyCheck.defaultFont')}
              {!font.own_font && (
                <span className='text-muted-foreground'>
                  {' '}
                  ·{' '}
                  {t(
                    font.fallback_to_dialogue
                      ? 'readyCheck.dialogueFallback'
                      : 'readyCheck.defaultFallback',
                  )}
                </span>
              )}
            </li>
          ))}
        </ul>
      )
    case 'models':
      return <ModelsFix data={data} />
    case 'export':
      return (
        <p className='break-words'>
          {t(data.enabled ? 'qc.on' : 'qc.off')} ·{' '}
          {data.format?.toLocaleUpperCase(i18n.language) || t('readyCheck.notSet')} ·{' '}
          {data.folder || t('readyCheck.notSet')}
        </p>
      )
  }
}

function ModelsFix({ data }: { data: Extract<ReadyData, { kind: 'models' }> }) {
  const { t, i18n } = useTranslation()
  const downloads = useKoharuStore((state) => state.downloads)
  const busy = useIsMutating({ mutationKey: ['ready-model-download'] }) > 0
  const download = useMutation({
    mutationKey: ['ready-model-download'],
    mutationFn: () => call(commands.downloadReadyModels),
    onSettled: refreshReadyCheck,
  })
  const active = Object.values(downloads).filter((item) => item.state === 'running')
  const bytes = (value: number) =>
    new Intl.NumberFormat(i18n.language, {
      style: 'unit',
      unit: 'megabyte',
      maximumFractionDigits: 1,
    }).format(value / 1_000_000)
  return (
    <div className='grid gap-3'>
      <ul className='grid gap-2'>
        {data.models.map((model) => (
          <li key={model.stage} className='flex flex-wrap items-center gap-x-2 gap-y-1'>
            <Status status={model.downloaded ? 'ready' : 'missing'} />
            <span className='min-w-0 break-words'>
              {t(`phase.${model.stage}`)}: {model.name}
            </span>
            <span className='text-muted-foreground'>
              {model.size_bytes === null
                ? t('readyCheck.sizeUnknown', { size: bytes(model.cached_bytes) })
                : bytes(model.size_bytes)}
            </span>
          </li>
        ))}
      </ul>
      {busy &&
        active.map((item) => (
          <div key={item.id} className='grid gap-1' role='status'>
            <span className='break-words'>
              {item.name} · {bytes(item.completed)}
              {item.total > 0 && ` / ${bytes(item.total)}`}
            </span>
            <Progress
              aria-label={item.name || t('activity.modelDownload')}
              value={item.total > 0 ? Math.min(100, (item.completed / item.total) * 100) : null}
            />
          </div>
        ))}
      {download.isError && (
        <p role='alert' className='text-destructive'>
          {t('readyCheck.downloadError')}
        </p>
      )}
      <Button
        size='sm'
        variant='outline'
        disabled={busy || data.models.every((model) => model.downloaded)}
        onClick={() => download.mutate()}
      >
        {t(busy ? 'readyCheck.downloading' : 'readyCheck.downloadAll')}
      </Button>
    </div>
  )
}
