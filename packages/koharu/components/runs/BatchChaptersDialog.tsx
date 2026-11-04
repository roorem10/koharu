'use client'

import { Channel } from '@tauri-apps/api/core'
import { useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { presetLabel, useRunSettings } from '@/components/runs/data'
import { playFinishSound, prepareFinishSound } from '@/components/runs/sound'
import { call } from '@/lib/backend'
import { pageKey, pagesKey, projectKey, refresh, useProject } from '@/lib/queries'
import { commands, type BatchPlan, type BatchProgress } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@koharu/ui/components/select'

export function BatchChaptersDialog({
  open,
  onOpenChange,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const { t } = useTranslation()
  const settings = useRunSettings()
  const project = useProject().data
  const [plan, setPlan] = useState<BatchPlan | null>(null)
  const [selected, setSelected] = useState<string[]>([])
  const [preset, setPreset] = useState('0')
  const [busy, setBusy] = useState(false)
  const [progress, setProgress] = useState<BatchProgress | null>(null)
  const [confirmClose, setConfirmClose] = useState(false)
  const [stopping, setStopping] = useState(false)
  const channel = useRef<Channel<BatchProgress> | null>(null)
  const running = busy || !!progress?.running
  const pick = async () => {
    setBusy(true)
    try {
      const next = await call(commands.previewChapterBatch)
      if (next) {
        setPlan(next)
        setSelected(
          next.chapters
            .filter((chapter) => !chapter.error && chapter.pages > 0)
            .map((chapter) => chapter.id),
        )
        setProgress(null)
      }
    } finally {
      setBusy(false)
    }
  }
  const start = async (closeProject: string | null) => {
    if (!plan) return
    prepareFinishSound()
    setConfirmClose(false)
    setBusy(true)
    setStopping(false)
    let finished = false
    const updates = new Channel<BatchProgress>()
    updates.onmessage = (next) => {
      setProgress(next)
      if (!next.running && !finished) {
        finished = true
        if (!next.stopped && next.finish_sound)
          playFinishSound(
            next.chapters.some(
              (chapter) => chapter.state === 'failed' || chapter.state === 'done_with_problems',
            ),
          )
        void refresh(projectKey, pagesKey, pageKey)
      }
    }
    channel.current = updates
    try {
      await call(commands.runChapterBatch, plan.id, selected, Number(preset), closeProject, updates)
    } finally {
      setBusy(false)
    }
  }
  const done =
    progress?.chapters.filter(
      (chapter) => chapter.state === 'done' || chapter.state === 'done_with_problems',
    ).length ?? 0
  const problems =
    progress?.chapters.filter((chapter) => chapter.state === 'done_with_problems').length ?? 0
  const failed = progress?.chapters.filter((chapter) => chapter.state === 'failed').length ?? 0
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!running) onOpenChange(next)
      }}
    >
      <DialogContent className='sm:max-w-xl' showCloseButton={false}>
        <DialogHeader>
          <DialogTitle>{t('runs.batchTitle')}</DialogTitle>
          <DialogDescription>{t('runs.batchDescription')}</DialogDescription>
        </DialogHeader>
        {confirmClose ? (
          <div className='grid gap-3'>
            <p>{t('runs.closeFirst', { project: project?.name })}</p>
            <Button onClick={() => void start(project?.name ?? null).catch(() => undefined)}>
              {t('runs.closeAndStart')}
            </Button>
            <Button variant='outline' onClick={() => setConfirmClose(false)}>
              {t('runs.cancel')}
            </Button>
          </div>
        ) : (
          <>
            {!progress?.running && (
              <Button
                variant='outline'
                disabled={busy}
                onClick={() => void pick().catch(() => undefined)}
              >
                {t(busy ? 'runs.scanning' : 'runs.pickFolder')}
              </Button>
            )}
            {progress ? (
              <div className='grid gap-2'>
                <ul
                  className='max-h-72 space-y-2 overflow-auto'
                  aria-label={t('runs.batchProgress')}
                >
                  {progress.chapters.map((chapter) => (
                    <li key={chapter.chapter.id} className='rounded-lg border p-2 text-xs'>
                      <div className='flex justify-between gap-3'>
                        <span className='min-w-0 break-words'>{chapter.chapter.name}</span>
                        <span className='shrink-0'>{t(`runs.chapterStates.${chapter.state}`)}</span>
                      </div>
                      {chapter.error && (
                        <p className='mt-1 break-words text-destructive'>{chapter.error}</p>
                      )}
                      {chapter.failures.map((failure) => (
                        <p key={failure.page} className='mt-1 break-words text-muted-foreground'>
                          {failure.label}: {t(`phase.${failure.stage}`)}: {failure.error}
                        </p>
                      ))}
                    </li>
                  ))}
                </ul>
                {!progress.running && (
                  <p role='status' className='text-sm'>
                    {t(progress.stopped ? 'runs.batchStoppedSummary' : 'runs.batchSummary', {
                      done,
                      problems,
                      failed,
                      total: progress.chapters.length,
                    })}
                  </p>
                )}
              </div>
            ) : (
              plan && (
                <div className='grid gap-3'>
                  <ul className='max-h-64 space-y-2 overflow-auto' aria-label={t('runs.chapters')}>
                    {plan.chapters.map((chapter) => (
                      <li key={chapter.id} className='rounded-lg border p-2 text-xs'>
                        <label className='flex items-center gap-2'>
                          <Checkbox
                            checked={selected.includes(chapter.id)}
                            disabled={!!chapter.error || chapter.pages === 0 || running}
                            onCheckedChange={(checked) =>
                              setSelected((current) =>
                                checked
                                  ? [...current, chapter.id]
                                  : current.filter((id) => id !== chapter.id),
                              )
                            }
                          />
                          <span className='min-w-0 flex-1 break-words'>{chapter.name}</span>
                          <span>{t('runs.pageCount', { count: chapter.pages })}</span>
                        </label>
                        {chapter.error && (
                          <p className='mt-1 break-words text-destructive'>{chapter.error}</p>
                        )}
                      </li>
                    ))}
                  </ul>
                  {plan.chapters.length === 0 && <p>{t('runs.noChapters')}</p>}
                  <Select
                    value={preset}
                    disabled={running}
                    onValueChange={(value) => value !== null && setPreset(value)}
                  >
                    <SelectTrigger aria-label={t('runs.batchPreset')}>
                      <SelectValue>
                        {settings.data?.presets[Number(preset)]
                          ? presetLabel(settings.data.presets[Number(preset)].name, t)
                          : t('runs.choosePreset')}
                      </SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                      {settings.data?.presets.map((preset, index) => (
                        <SelectItem key={index} value={String(index)}>
                          {presetLabel(preset.name, t)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </div>
              )
            )}
            <DialogFooter>
              {progress?.running ? (
                <Button
                  variant='outline'
                  disabled={stopping}
                  onClick={() => {
                    setStopping(true)
                    void call(commands.stopChapterBatch).catch(() => setStopping(false))
                  }}
                >
                  {t(stopping ? 'runs.stoppingAfterPage' : 'runs.stopBatch')}
                </Button>
              ) : (
                <>
                  <DialogClose render={<Button variant='outline' disabled={busy} />}>
                    {t('runs.close')}
                  </DialogClose>
                  {!progress && (
                    <Button
                      disabled={
                        running ||
                        !plan ||
                        selected.length === 0 ||
                        !settings.data?.presets[Number(preset)]
                      }
                      onClick={() => {
                        if (project) setConfirmClose(true)
                        else void start(null).catch(() => undefined)
                      }}
                    >
                      {t('runs.startBatch')}
                    </Button>
                  )}
                </>
              )}
            </DialogFooter>
          </>
        )}
      </DialogContent>
    </Dialog>
  )
}
