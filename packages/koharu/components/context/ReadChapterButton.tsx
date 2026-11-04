'use client'

import { useQuery } from '@tanstack/react-query'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { call } from '@/lib/backend'
import { queryClient, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type ModelSelection, type Scope } from '@koharu/bridge/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@koharu/ui/components/dialog'

import { contextSettingsKey, refreshContext, useChapterContext, useContextSettings } from './data'
import { ReadingModelPicker } from './ReadingModelPicker'
import { SeriesPicker } from './SeriesPicker'

export function ReadChapterButton({
  scope,
  beforeTranslation = false,
}: {
  scope: Scope
  beforeTranslation?: boolean
}) {
  const { t } = useTranslation()
  const project = useProject().data
  const chapter = useChapterContext().data
  const settings = useContextSettings()
  const translation = useKoharuStore((state) => state.preferences?.pipeline.translation.model)
  const jobs = useKoharuStore((state) => state.jobs)
  const running = Object.values(jobs).some((job) => job.state === 'running')
  const model = useQuery({
    queryKey: ['reading-model', settings.data?.read_model, translation],
    queryFn: () => call(commands.getReadingModel),
  })
  const [step, setStep] = useState<'series' | 'confirm' | 'separate' | 'pick' | null>(null)
  const [picked, setPicked] = useState<ModelSelection | null>(null)
  const [dontAsk, setDontAsk] = useState(false)
  const [busy, setBusy] = useState(false)
  const [openedFor, setOpenedFor] = useState<string | undefined>()
  const close = () => {
    setStep(null)
    setPicked(null)
    setDontAsk(false)
  }
  const start = async (selection: ModelSelection | null, saveModel = false) => {
    if (!settings.data || !project) return
    const name = project.name
    setBusy(true)
    try {
      if (dontAsk || saveModel) {
        const saved = await call(commands.saveContextSettings, {
          ...settings.data,
          ask_before_reading: dontAsk ? false : settings.data.ask_before_reading,
          read_model: saveModel ? selection : settings.data.read_model,
        })
        queryClient.setQueryData(contextSettingsKey, saved)
      }
      if (queryClient.getQueryData<{ name: string }>(['project'])?.name !== name) return
      await call(commands.readChapter, scope, selection)
      close()
      await refreshContext()
    } catch {
      // call reports the error and leaves the dialog available for retry.
    } finally {
      setBusy(false)
    }
  }
  const confirm = () => {
    if (!model.data) return
    if (!model.data.vision) setStep('separate')
    else if (!settings.data?.ask_before_reading) void start(model.data.selection)
    else setStep('confirm')
  }
  return (
    <>
      <Button
        size='sm'
        variant={beforeTranslation ? 'ghost' : 'default'}
        className={beforeTranslation ? 'h-auto flex-wrap justify-start text-xs' : 'w-full'}
        disabled={!project || running || busy || !settings.data || !model.data}
        onClick={() => {
          setOpenedFor(project?.name)
          setDontAsk(false)
          if (!chapter?.series) setStep('series')
          else confirm()
        }}
      >
        {t('chapterContext.readChapter')}
        {beforeTranslation && (
          <span className='text-[10px] text-muted-foreground'>
            {t('chapterContext.beforeTranslation')}
          </span>
        )}
      </Button>
      <Dialog
        open={step !== null && openedFor === project?.name}
        onOpenChange={(open) => {
          if (!open && !busy) close()
        }}
      >
        <DialogContent showCloseButton={false} className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle>{t('chapterContext.readChapter')}</DialogTitle>
            <DialogDescription>
              {t(
                step === 'series'
                  ? 'chapterContext.chooseSeriesHint'
                  : step === 'pick'
                    ? 'chapterContext.pickReader'
                    : step === 'separate'
                      ? 'chapterContext.noVisionConfirm'
                      : model.data?.local
                        ? 'chapterContext.localConfirm'
                        : 'chapterContext.cloudConfirm',
                { model: model.data?.label },
              )}
            </DialogDescription>
          </DialogHeader>
          {step === 'series' && (
            <SeriesPicker
              value={chapter?.series?.id ?? null}
              onChange={async (id) => {
                await call(commands.setProjectSeries, id)
                await refreshContext()
                confirm()
              }}
            />
          )}
          {step === 'pick' && (
            <ReadingModelPicker
              value={picked}
              onSelect={setPicked}
              onBack={() => setStep('separate')}
            />
          )}
          {step === 'confirm' && (
            <label className='flex items-center gap-2 text-xs'>
              <Checkbox checked={dontAsk} onCheckedChange={(value) => setDontAsk(value === true)} />
              {t('chapterContext.dontAsk')}
            </label>
          )}
          <DialogFooter>
            <Button size='sm' variant='outline' disabled={busy} onClick={close}>
              {t(
                step === 'pick' || step === 'series'
                  ? 'chapterContext.cancel'
                  : 'chapterContext.no',
              )}
            </Button>
            {step === 'separate' && (
              <Button size='sm' onClick={() => setStep('pick')}>
                {t('chapterContext.yes')}
              </Button>
            )}
            {step === 'confirm' && (
              <Button
                size='sm'
                disabled={busy || running}
                onClick={() => void start(model.data?.selection ?? null)}
              >
                {t('chapterContext.yes')}
              </Button>
            )}
            {step === 'pick' && (
              <Button
                size='sm'
                disabled={busy || !picked || running}
                onClick={() => void start(picked, true)}
              >
                {t('chapterContext.run')}
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  )
}
