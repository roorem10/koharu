'use client'

import { useIsMutating } from '@tanstack/react-query'
import { useEffect, useMemo, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { useDebounce } from 'use-debounce'

import { NumberField } from '@/components/preferences/PreferenceFields'
import {
  closeLevels,
  completeLevels,
  fixes,
  fixLevelsKey,
  isClipPercent,
  isThreshold,
  MAX_CLIP_PERCENT,
  skipReason,
  stopLevels,
  useFixLevels,
  useLevels,
  useLevelsPreview,
  useLevelsProgress,
  usePngUrl,
  type Levels,
  type LevelsScope,
} from '@/lib/booharu/levels'
import { useBooharuPreferences } from '@/lib/booharu/preferences'
import { useProject } from '@/lib/queries'
import type { LevelsReport, PageId, PageSummary } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import { RadioGroup, RadioGroupItem } from '@koharu/ui/components/radio-group'
import { Switch } from '@koharu/ui/components/switch'

/** The pages shown nowhere, while no project is open. */
const noPages: PageSummary[] = []

/** Fix page levels, ported from Booharu 0.0.4's page levels dialog: the pages to fix, the selected
 * ones or all, the four values, starting from the saved `[booharu-levels]`, which nothing here saves,
 * and the first page's black and white points with its scan before and after. Fix runs as a row of
 * the Activity center, with a progress line and Stop here; then the dialog lists what it changed
 * and what it skipped, and why. Mounted once, in the editor, in both modes: the page menu and the
 * Edit menu open it. It closes when its project does, or another opens, and not while it fixes. */
export function LevelsDialog() {
  const selected = useLevels((state) => state.selected)
  const opened = useLevels((state) => state.project)
  const project = useProject().data?.name ?? null
  const fixing = useIsMutating({ mutationKey: fixLevelsKey }) > 0
  useLevelsProgress()

  useEffect(() => {
    if (selected !== null && project !== opened) closeLevels()
  }, [selected, project, opened])

  return (
    <Dialog
      open={selected !== null}
      onOpenChange={(open) => {
        if (!open && !fixing) closeLevels()
      }}
    >
      <DialogContent className='max-h-[90vh] overflow-auto sm:max-w-3xl' showCloseButton={false}>
        {selected !== null && <LevelsBody selected={selected} fixing={fixing} />}
      </DialogContent>
    </Dialog>
  )
}

/** The open dialog: its settings and preview, or once Fix ends, what it did. */
function LevelsBody({ selected, fixing }: { selected: PageId[]; fixing: boolean }) {
  const { t } = useTranslation()
  const pages = useProject().data?.pages ?? noPages
  const opened = useLevels((state) => state.scope)
  const progress = useLevels((state) => state.progress)
  const preferences = useBooharuPreferences()
  const saved = useMemo(
    () => (preferences.data ? completeLevels(preferences.data['booharu-levels']) : undefined),
    [preferences.data],
  )
  const [draft, setDraft] = useState<Levels | null>(null)
  const levels = draft ?? saved
  const chosen = useMemo(
    () => selected.filter((page) => pages.some((each) => each.id === page)),
    [selected, pages],
  )
  const [scope, setScope] = useState<LevelsScope>(
    opened === 'selected' && chosen.length > 0 ? 'selected' : 'all',
  )
  const targets = useMemo(
    () => (scope === 'all' ? pages.map((page) => page.id) : chosen),
    [scope, pages, chosen],
  )
  const fix = useFixLevels()
  const report = fix.data ?? null
  const stopped = fix.isSuccess && fix.data === null

  const name = (page: PageId) => {
    const index = pages.findIndex((each) => each.id === page)
    return index < 0 ? page : t('navigator.pageNumber', { number: index + 1 })
  }

  return (
    <>
      <DialogHeader>
        <DialogTitle>{t('booharu.levels.fixTitle')}</DialogTitle>
        <DialogDescription>{t('booharu.levels.description')}</DialogDescription>
      </DialogHeader>
      {report ? (
        <LevelsResult report={report} name={name} />
      ) : !levels ? (
        <p role='status' className='text-xs'>
          {t(preferences.isError ? 'booharu.levels.loadError' : 'booharu.levels.loading')}
        </p>
      ) : (
        <>
          <LevelsFields value={levels} disabled={fix.isPending} onChange={setDraft} />
          <RadioGroup
            aria-label={t('booharu.levels.scope')}
            value={scope}
            disabled={fix.isPending}
            className='flex flex-wrap gap-4'
            onValueChange={(next) => setScope(next === 'selected' ? 'selected' : 'all')}
          >
            <label className='flex items-center gap-2 text-xs'>
              <RadioGroupItem value='selected' disabled={chosen.length === 0} />
              {t('booharu.levels.selected')}
            </label>
            <label className='flex items-center gap-2 text-xs'>
              <RadioGroupItem value='all' />
              {t('booharu.levels.all')}
            </label>
          </RadioGroup>
          <p className='text-xs text-muted-foreground'>{t('booharu.levels.skipHelp')}</p>
          <LevelsPreviewPane page={targets[0] ?? null} name={name} levels={levels} />
          {fix.isPending && (
            <div className='flex items-center justify-between gap-3'>
              <p role='status' className='text-xs'>
                {progress
                  ? t('booharu.levels.progress', { done: progress.done, total: progress.total })
                  : t('booharu.levels.fixing')}
              </p>
              <Button variant='outline' size='sm' onClick={stopLevels}>
                {t('activity.stop')}
              </Button>
            </div>
          )}
          {stopped && (
            <p role='status' className='text-xs text-muted-foreground'>
              {t('booharu.levels.stopped')}
            </p>
          )}
        </>
      )}
      <DialogFooter>
        <Button variant='outline' disabled={fixing} onClick={closeLevels}>
          {t(report ? 'booharu.levels.done' : 'common.cancel')}
        </Button>
        {!report && (
          <Button
            disabled={!levels || !fixes(levels) || targets.length === 0 || fixing}
            onClick={() => {
              if (levels) fix.mutate({ pages: targets, levels })
            }}
          >
            {t(fix.isPending ? 'booharu.levels.fixing' : 'booharu.levels.apply')}
          </Button>
        )}
      </DialogFooter>
    </>
  )
}

/** The four values: Auto levels with how much it clips at each end, and Whiten paper with its
 * threshold. A cleared or half-typed number changes nothing. */
function LevelsFields({
  value,
  disabled,
  onChange,
}: {
  value: Levels
  disabled: boolean
  onChange: (levels: Levels) => void
}) {
  const { t } = useTranslation()
  return (
    <fieldset disabled={disabled} className='grid gap-3'>
      <div className='grid grid-cols-2 items-end gap-4'>
        <div className='flex items-center gap-2 text-xs'>
          <Switch
            aria-label={t('booharu.levels.autoLevels')}
            checked={value.auto_levels}
            disabled={disabled}
            onCheckedChange={(auto_levels) => onChange({ ...value, auto_levels })}
          />
          <span aria-hidden='true'>{t('booharu.levels.autoLevels')}</span>
        </div>
        <NumberField
          label={t('booharu.levels.clip')}
          value={value.clip_percent}
          min={0}
          max={MAX_CLIP_PERCENT}
          step={0.1}
          onChange={(clip_percent) => {
            if (isClipPercent(clip_percent)) onChange({ ...value, clip_percent })
          }}
        />
      </div>
      <div className='grid grid-cols-2 items-end gap-4'>
        <div className='flex items-center gap-2 text-xs'>
          <Switch
            aria-label={t('booharu.levels.whiten')}
            checked={value.whiten}
            disabled={disabled}
            onCheckedChange={(whiten) => onChange({ ...value, whiten })}
          />
          <span aria-hidden='true'>{t('booharu.levels.whiten')}</span>
        </div>
        <NumberField
          label={t('booharu.levels.threshold')}
          value={value.threshold}
          min={0}
          max={255}
          step={1}
          integer
          onChange={(threshold) => {
            if (threshold === null) return
            const whole = Math.round(threshold)
            if (isThreshold(whole)) onChange({ ...value, threshold: whole })
          }}
        />
      </div>
    </fieldset>
  )
}

/** What the fix would do to `page`, the first page it takes: its black and white points and its
 * scan before and after, read again 200 ms after the values last changed. */
function LevelsPreviewPane({
  page,
  name,
  levels,
}: {
  page: PageId | null
  name: (page: PageId) => string
  levels: Levels
}) {
  const { t } = useTranslation()
  const [settled] = useDebounce(levels, 200)
  const preview = useLevelsPreview(page, settled)
  const pending = settled !== levels || preview.isFetching
  const before = usePngUrl(pending ? undefined : preview.data?.before)
  const after = usePngUrl(pending ? undefined : preview.data?.after)

  const status = !fixes(levels)
    ? t('booharu.levels.nothingToFix')
    : page === null
      ? t('booharu.levels.noPages')
      : pending || !preview.data
        ? t('booharu.levels.loadingPreview')
        : null

  return (
    <section className='grid gap-2' aria-label={t('booharu.levels.preview')} aria-busy={pending}>
      {page !== null && (
        <p className='text-xs text-muted-foreground'>
          {t('booharu.levels.previewPage', { page: name(page) })}
        </p>
      )}
      {preview.isError && fixes(levels) ? (
        <p role='alert' className='text-xs text-destructive'>
          {t('booharu.levels.previewError')}
        </p>
      ) : status !== null ? (
        <p role='status' className='py-8 text-center text-xs text-muted-foreground'>
          {status}
        </p>
      ) : (
        <div className='grid grid-cols-2 gap-3'>
          {(
            [
              ['before', before],
              ['after', after],
            ] as const
          ).map(([side, url]) => (
            <figure key={side} className='min-w-0 rounded-lg border bg-muted/30 p-2'>
              <figcaption className='mb-1 text-center text-xs font-medium'>
                {t(`booharu.levels.${side}`)}
              </figcaption>
              {url && (
                <img
                  src={url}
                  alt={t(`booharu.levels.${side}`)}
                  className='mx-auto h-64 max-h-[35vh] w-full object-contain'
                />
              )}
            </figure>
          ))}
        </div>
      )}
      {levels.auto_levels && preview.data && status === null && !preview.isError && (
        <p className='min-w-0 text-xs break-all text-muted-foreground'>
          {t('booharu.levels.points', { black: preview.data.black, white: preview.data.white })}
        </p>
      )}
    </section>
  )
}

/** What a fix did: how many pages it changed, which, and each page it skipped, with why. */
function LevelsResult({ report, name }: { report: LevelsReport; name: (page: PageId) => string }) {
  const { t } = useTranslation()
  return (
    <div role='status' className='grid gap-2 text-xs'>
      <p>{t('booharu.levels.fixed', { count: report.changed.length })}</p>
      {report.changed.length > 0 && (
        <p className='max-h-24 overflow-auto break-all text-muted-foreground'>
          {report.changed.map(name).join(', ')}
        </p>
      )}
      {report.skipped.length > 0 && (
        <>
          <p>{t('booharu.levels.skipped', { count: report.skipped.length })}</p>
          <ul className='max-h-52 space-y-1 overflow-auto'>
            {report.skipped.map(({ page, reason }) => (
              <li key={page} className='break-all'>
                {t('booharu.levels.skipDetail', {
                  page: name(page),
                  reason: t(skipReason(reason)),
                })}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  )
}
