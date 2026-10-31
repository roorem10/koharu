'use client'

import { useQuery } from '@tanstack/react-query'
import { Channel } from '@tauri-apps/api/core'
import {
  AlertTriangle,
  BookOpenCheck,
  Languages,
  LoaderCircle,
  ScanText,
  Shrink,
  Type,
  UnfoldVertical,
} from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { refreshContext } from '@/components/context/data'
import { call } from '@/lib/backend'
import {
  pageKey,
  preparedPageKey,
  projectKey,
  queryClient,
  usePage,
  usePages,
  useProject,
} from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { showCanvasPage } from '@koharu/bridge/canvas'
import {
  commands,
  type CanvasPagePreparation,
  type Flag,
  type Page,
  type ProjectInfo,
  type QcCheck,
  type QcFinding,
  type QcProgress,
  type QcReport,
  type QcScope,
} from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import { Input } from '@koharu/ui/components/input'
import { ScrollArea } from '@koharu/ui/components/scroll-area'

const icons = {
  overflow: UnfoldVertical,
  too_small: Shrink,
  missing_translation: Languages,
  empty_source: ScanText,
  leftover_source: Languages,
  odd_font: Type,
  flag: BookOpenCheck,
} satisfies Record<QcCheck, typeof Type>
const settingsKey = ['qc-settings'] as const

export function QcPanel() {
  const { t } = useTranslation()
  const project = useProject().data
  const page = usePage().data
  const pages = usePages(false).data ?? []
  const [report, setReport] = useState<QcReport | null>(null)
  const [showHidden, setShowHidden] = useState(false)
  const [busy, setBusy] = useState(false)
  const [progress, setProgress] = useState<QcProgress | null>(null)
  const [minimum, setMinimum] = useState('12')
  const [checkedMinimum, setCheckedMinimum] = useState<number | null>(null)
  const request = useRef(0)
  const active = useRef(false)
  const settingsWrites = useRef<Promise<unknown>>(Promise.resolve())
  const settingsGeneration = useRef(0)
  const settings = useQuery({ queryKey: settingsKey, queryFn: () => call(commands.getQcSettings) })
  useEffect(() => {
    if (settings.data) setMinimum(String(settings.data.min_font_size ?? 12))
  }, [settings.data])
  useEffect(() => {
    request.current += 1
    active.current = false
    setBusy(false)
    setProgress(null)
    setReport(null)
    setShowHidden(false)
    return () => {
      request.current += 1
    }
  }, [project?.name])

  const saveMinimum = async () => {
    const value = Number(minimum)
    if (!Number.isFinite(value) || value <= 0) {
      setMinimum(String(settings.data?.min_font_size ?? 12))
      return settings.data?.min_font_size ?? 12
    }
    const generation = ++settingsGeneration.current
    const saved = settingsWrites.current
      .catch(() => undefined)
      .then(() => call(commands.saveQcSettings, { min_font_size: value }))
    settingsWrites.current = saved
    await saved
    if (settingsGeneration.current === generation)
      queryClient.setQueryData(settingsKey, { min_font_size: value })
    return value
  }

  const check = async (scope: QcScope) => {
    if (!project || active.current) return
    active.current = true
    const token = ++request.current
    setBusy(true)
    setProgress(null)
    try {
      const min = await saveMinimum()
      if (request.current !== token) return
      const channel = new Channel<QcProgress>()
      channel.onmessage = (value) => {
        if (request.current === token) setProgress(value)
      }
      const result = await call(commands.runQc, scope, channel)
      if (request.current === token) {
        setReport(result)
        setCheckedMinimum(min)
      }
    } catch {
      // call() reports native errors; retain the previous results for review.
    } finally {
      if (request.current === token) {
        active.current = false
        setBusy(false)
        setProgress(null)
      }
    }
  }

  const navigate = async (finding: QcFinding) => {
    if (active.current) return
    active.current = true
    setBusy(true)
    const token = ++request.current
    const previousProject = queryClient.getQueryData<ProjectInfo | null>(projectKey)
    const previousPage = queryClient.getQueryData<Page | null>(pageKey)
    const prepared = queryClient.getQueryData<CanvasPagePreparation>(preparedPageKey(finding.page))
    const activated = showCanvasPage(finding.page, previousProject?.revision ?? null)
    if (activated && previousProject && prepared?.revision === previousProject.revision) {
      queryClient.setQueryData(projectKey, { ...previousProject, active_page: finding.page })
      queryClient.setQueryData(pageKey, prepared.page)
    }
    useKoharuStore.getState().selectPages([finding.page])
    useKoharuStore.getState().selectLayers([])
    try {
      const selection = await call(commands.selectPage, finding.page)
      if (request.current !== token) return
      queryClient.setQueryData(projectKey, selection.project)
      queryClient.setQueryData(pageKey, selection.page)
      useKoharuStore.getState().selectLayers([finding.layer])
    } catch {
      if (
        request.current === token &&
        queryClient.getQueryData<ProjectInfo | null>(projectKey)?.active_page === finding.page
      ) {
        queryClient.setQueryData(projectKey, previousProject)
        queryClient.setQueryData(pageKey, previousPage)
        useKoharuStore.getState().selectPages(previousPage ? [previousPage.id] : [])
      }
    } finally {
      if (request.current === token) {
        active.current = false
        setBusy(false)
      }
    }
  }

  const mark = async (finding: QcFinding, hidden: boolean) => {
    if (active.current) return
    active.current = true
    setBusy(true)
    const token = ++request.current
    try {
      if (finding.check === 'flag' && finding.flag) {
        // Indices change when another flag is dismissed. Resolve this flag against the current sidecar.
        const chapter = await call(commands.getChapterContext)
        if (request.current !== token) return
        const flags = chapter.context.lines?.[finding.layer]?.flags ?? []
        const index = flags.findIndex((flag) => sameFlag(flag, finding.flag!))
        if (index >= 0) await call(commands.dismissLineFlag, finding.layer, index)
        await refreshContext()
      } else {
        await call(commands.markQcOk, finding.layer, finding.check, !hidden)
      }
      if (request.current !== token) return
      setReport((current) => {
        if (!current) return current
        const findings = current.findings.filter((item) => item !== finding)
        const hiddenFindings = current.hidden.filter((item) => item !== finding)
        if (finding.check !== 'flag') (hidden ? findings : hiddenFindings).push(finding)
        return { ...current, findings, hidden: hiddenFindings }
      })
    } catch {
      // Keep the entry if the acknowledgement could not be saved.
    } finally {
      if (request.current === token) {
        active.current = false
        setBusy(false)
      }
    }
  }

  const entries = [
    ...(report?.findings ?? []).map((finding) => ({ finding, hidden: false })),
    ...(showHidden ? (report?.hidden ?? []) : []).map((finding) => ({ finding, hidden: true })),
  ]
  const groups = new Map<string, typeof entries>()
  for (const entry of entries) {
    const group = groups.get(entry.finding.page) ?? []
    group.push(entry)
    groups.set(entry.finding.page, group)
  }
  const stale =
    report &&
    (report.revision !== project?.revision ||
      checkedMinimum !== (settings.data?.min_font_size ?? 12))
  const label = (finding: QcFinding) => {
    if (finding.flag?.kind === 'review') return finding.detail
    if (finding.flag?.kind === 'glossary') return t('qc.glossaryFlag', { term: finding.detail })
    return t(`qc.checks.${finding.check}`, { detail: finding.detail || t('qc.defaultFont') })
  }

  return (
    <section
      className='flex min-h-0 min-w-0 flex-1 flex-col'
      aria-label={t('qc.title')}
      aria-busy={busy}
    >
      <fieldset disabled={busy} className='flex min-h-0 min-w-0 flex-1 flex-col'>
        <div className='grid shrink-0 gap-2 border-b border-border/80 p-3'>
          <div className='flex gap-2'>
            <Button
              className='h-7 min-w-0 flex-1 px-2 text-[10px]'
              size='sm'
              variant='outline'
              disabled={!page || settings.isPending || settings.isError}
              onClick={() => page && void check({ scope: 'page', page: page.id })}
            >
              {t('qc.checkPage')}
            </Button>
            <Button
              className='h-7 min-w-0 flex-1 px-2 text-[10px]'
              size='sm'
              variant='outline'
              disabled={!project || settings.isPending || settings.isError}
              onClick={() => void check({ scope: 'project' })}
            >
              {t('qc.checkChapter')}
            </Button>
          </div>
          <label className='flex items-center gap-2 text-[10px] text-muted-foreground'>
            <Checkbox
              disabled={busy}
              checked={showHidden}
              onCheckedChange={(checked) => setShowHidden(checked === true)}
            />
            {t('qc.showHidden')}
          </label>
          {stale && (
            <p className='flex items-start gap-1 text-[10px] text-amber-600 dark:text-amber-400'>
              <AlertTriangle className='size-3 shrink-0' />
              {t('qc.stale')}
            </p>
          )}
        </div>
        {busy && (
          <div
            role='status'
            className='flex shrink-0 items-center gap-2 px-3 py-2 text-[10px] text-muted-foreground'
          >
            <LoaderCircle className='size-3 animate-spin' />
            {progress
              ? t('qc.progress', { completed: progress.completed, total: progress.total })
              : t('qc.working')}
          </div>
        )}
        <ScrollArea className='min-h-0 flex-1'>
          {!entries.length && (
            <p className='p-3 text-[11px] leading-relaxed text-muted-foreground'>
              {report ? t('qc.noFindings') : t('qc.hint')}
            </p>
          )}
          {[...groups.entries()]
            .sort(([left], [right]) => {
              const leftIndex = pages.findIndex((page) => page.id === left)
              const rightIndex = pages.findIndex((page) => page.id === right)
              return (
                (leftIndex < 0 ? Infinity : leftIndex) - (rightIndex < 0 ? Infinity : rightIndex)
              )
            })
            .map(([id, entries]) => (
              <div key={id} className='pb-2'>
                <h3
                  className='truncate bg-[var(--surface-well)] px-3 py-2 text-[10px] font-semibold'
                  title={entries[0]!.finding.page_label}
                >
                  {entries[0]!.finding.page_label}
                </h3>
                {entries.map(({ finding, hidden }, index) => {
                  const Icon = icons[finding.check]
                  const text = label(finding)
                  return (
                    <div
                      key={`${finding.layer}:${finding.check}:${index}`}
                      className={`flex min-w-0 items-center gap-1 border-b border-border/40 px-2 py-1.5 ${hidden ? 'opacity-60' : ''}`}
                    >
                      <button
                        type='button'
                        className='flex min-w-0 flex-1 items-start gap-2 rounded-md p-1 text-left hover:bg-foreground/5 disabled:pointer-events-none'
                        onClick={() => void navigate(finding)}
                      >
                        <Icon
                          className='mt-0.5 size-3.5 shrink-0 text-muted-foreground'
                          aria-hidden
                        />
                        <span className='min-w-0 flex-1'>
                          <span className='block truncate text-[11px]' title={text}>
                            {text}
                          </span>
                          <span
                            className='block truncate text-[10px] text-muted-foreground'
                            title={finding.translation}
                          >
                            {finding.translation || t('qc.noTranslation')}
                          </span>
                        </span>
                      </button>
                      <Button
                        size='sm'
                        variant='ghost'
                        className='h-6 shrink-0 px-1.5 text-[10px]'
                        aria-label={t(hidden ? 'qc.undoFor' : 'qc.okFor', { check: text })}
                        onClick={() => void mark(finding, hidden)}
                      >
                        {t(hidden ? 'qc.undoOk' : 'qc.ok')}
                      </Button>
                    </div>
                  )
                })}
              </div>
            ))}
        </ScrollArea>
        <div className='shrink-0 border-t border-border/80 p-3'>
          <label className='flex items-center gap-2 text-[10px] text-muted-foreground'>
            <span className='min-w-0 flex-1'>{t('qc.minimumFontSize')}</span>
            <Input
              type='number'
              min={1}
              step={1}
              value={minimum}
              className='h-7 w-16 text-xs'
              disabled={settings.isPending || settings.isError}
              onChange={(event) => setMinimum(event.currentTarget.value)}
              onBlur={() => void saveMinimum().catch(() => undefined)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') event.currentTarget.blur()
                if (event.key === 'Escape') setMinimum(String(settings.data?.min_font_size ?? 12))
              }}
            />
          </label>
          {settings.isError && (
            <p className='mt-2 text-[10px] text-destructive'>
              {t('qc.settingsFailed')}{' '}
              <button type='button' className='underline' onClick={() => void settings.refetch()}>
                {t('qc.retry')}
              </button>
            </p>
          )}
        </div>
      </fieldset>
    </section>
  )
}

function sameFlag(left: Flag, right: Flag): boolean {
  return (
    left.kind === right.kind &&
    left.page === right.page &&
    (left.kind === 'review' && right.kind === 'review'
      ? left.message === right.message
      : left.kind === 'glossary' && right.kind === 'glossary' && left.term === right.term)
  )
}
