'use client'

import type { TFunction } from 'i18next'
import { BookOpenText, LoaderCircle } from 'lucide-react'
import { useState } from 'react'
import { useTranslation } from 'react-i18next'

import { ReadingModelPicker } from '@/components/booharu/context/ReadingModelPicker'
import { SeriesPicker } from '@/components/booharu/context/SeriesPicker'
import { call } from '@/lib/backend'
import { useReadingChapter } from '@/lib/booharu/booharuJobs'
import { linkSeries, readChapter } from '@/lib/booharu/context'
import { booharuPreferencesQuery } from '@/lib/booharu/preferences'
import { completeReading, isLocal, openProject, saveReading } from '@/lib/booharu/reading'
import { queryClient, useProject } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { modelName } from '@/lib/translation'
import { commands, type CantRead, type ReadSummary } from '@koharu/ipc/protocol'
import { Button } from '@koharu/ui/components/button'
import { Checkbox } from '@koharu/ui/components/checkbox'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogTitle,
} from '@koharu/ui/components/dialog'
import { toast } from '@koharu/ui/components/toast'

/** A step of pressing Read chapter, each a question in the dialog. */
type Step =
  /** The chapter links no series: pick or make one. */
  | { kind: 'series' }
  /** No model can read the pages, as `model`, the one a read would use, can't, for `why`: read
   * with a separate model? `ask` is whether to ask before reading. */
  | { kind: 'separate'; model: string; why: CantRead; ask: boolean }
  /** Choose the Read chapter model. */
  | { kind: 'pick'; ask: boolean }
  /** Read with `model`, on this computer when `local`? */
  | { kind: 'confirm'; model: string; local: boolean }

/** The string key asking whether to read with a separate model, as the model a read would use
 * can't, for `why`. */
const separately = (why: CantRead) =>
  why === 'no_vision' ? 'booharu.context.noVisionConfirm' : 'booharu.context.onlyTranslatesConfirm'

/** Says what a read did: the pages it read with its model, then each thing it found, wrote, kept or
 * skipped; or that it stopped, having saved nothing. */
function showRead(summary: ReadSummary, t: TFunction): void {
  if (summary.stopped) {
    toast.add({ type: 'info', title: t('booharu.context.readStopped') })
    return
  }
  const counted = (key: string, count: number) => (count > 0 ? t(key, { count }) : null)
  const notes = [
    counted('booharu.context.readCharacters', summary.characters),
    counted('booharu.context.readSuggestions', summary.suggestions_added),
    counted('booharu.context.readLabelled', summary.lines_labelled),
    counted('booharu.context.readNotes', summary.notes_written),
    summary.summary_kept ? t('booharu.context.readSummaryKept') : null,
    counted('booharu.context.readKeptLabels', summary.kept_user_labels),
    counted('booharu.context.fixKeptLocked', summary.skipped_locked),
    counted('booharu.context.readSkippedChanged', summary.skipped_changed),
    counted('booharu.context.readUnknownIds', summary.unknown_ids),
    counted('booharu.context.readBlankTerms', summary.blank_terms),
  ].filter((note) => note !== null)
  toast.add({
    type: 'success',
    title: t('booharu.context.readDone', {
      count: summary.pages,
      model: modelName(useKoharuStore.getState().models, summary.model),
    }),
    description: notes.join(' ') || undefined,
  })
}

/** The Context tab's Read chapter, Booharu 0.0.4's button: it reads the open project's whole
 * chapter, asking first what it needs. With no series linked, the series step: pick or make one,
 * which the chapter then links; Cancel stops. Then, from the model the desktop would read with:
 * with none, as the one it would use has no vision or only translates, whether to read with a
 * separate model, and on Yes the reading-model picker, whose pick is saved as the Read chapter
 * model; and while Ask before reading a chapter is on, whether to read, on this computer, which
 * takes a while, or with API credits, with Don't ask again, which turns it off. A read goes from
 * the button, the dialog closed: while one goes, by hand, by the agent or as a Run press's step,
 * the button says so, and the Activity center lists it with Stop. At the end a toast says what it
 * did; a refusal, such as with no text to read, shows its error, as every failed command's does.
 * The dialog shows only for the project it was pressed in, and a read starts only while that
 * project is open. `linked` is whether the chapter links a series that reads. */
export function ReadChapterButton({ linked }: { linked: boolean }) {
  const { t } = useTranslation()
  const project = useProject().data?.name ?? null
  const models = useKoharuStore((state) => state.models)
  const running = useKoharuStore((state) =>
    Object.values(state.jobs).some((job) => job.state === 'running'),
  )
  const going = useReadingChapter()
  const [step, setStep] = useState<Step | null>(null)
  const [pressedIn, setPressedIn] = useState<string | null>(null)
  const [dontAsk, setDontAsk] = useState(false)
  // Asking the desktop which model reads, or saving Don't ask again: the dialog waits for it.
  const [busy, setBusy] = useState(false)
  // This button's read, from its start till the desktop answers.
  const [reading, setReading] = useState(false)
  const close = () => setStep(null)

  /** Reads the chapter, the dialog closed, while `pressed`, the project the press was in, is
   * still open; then says what the read did. */
  const read = (pressed: string | null) => {
    close()
    if (pressed === null || openProject() !== pressed) return
    setReading(true)
    readChapter()
      .then((summary) => showRead(summary, t))
      .catch(() => undefined)
      .finally(() => setReading(false))
  }

  /** Asks before `model` reads, while `ask` is on; else reads. */
  const confirm = (pressed: string | null, model: string, local: boolean, ask: boolean) => {
    if (ask) setStep({ kind: 'confirm', model, local })
    else read(pressed)
  }

  /** Goes on once the chapter links a series: asks the desktop which model a read would use, and
   * the settings, as they are now, whether to ask before reading. */
  const next = async (pressed: string | null) => {
    setBusy(true)
    try {
      const [reader, preferences] = await Promise.all([
        call(commands.booharuGetReadingModel),
        queryClient.fetchQuery({ ...booharuPreferencesQuery, staleTime: 0 }),
      ])
      const { ask } = completeReading(preferences['booharu-reading'])
      if (reader.model !== null) {
        confirm(pressed, reader.model, reader.local, ask)
        return
      }
      // A saved model that can't read makes a read refuse rather than fall back, so it's the one
      // to name; else the translation model.
      const [model, why] =
        reader.saved !== null && reader.saved_cant_read !== null
          ? [reader.saved, reader.saved_cant_read]
          : [reader.translation, reader.translation_cant_read]
      setStep(why === null ? { kind: 'pick', ask } : { kind: 'separate', model, why, ask })
    } catch {
      // The failed read of the model or the settings has shown its error.
      close()
    } finally {
      setBusy(false)
    }
  }

  const press = () => {
    setPressedIn(project)
    setDontAsk(false)
    close()
    if (linked) void next(project)
    else setStep({ kind: 'series' })
  }

  const yes = () => {
    if (!dontAsk) {
      read(pressedIn)
      return
    }
    setBusy(true)
    // A failed save shows its error, and the dialog stays to try again.
    saveReading({ ask: false })
      .then(
        () => read(pressedIn),
        () => undefined,
      )
      .finally(() => setBusy(false))
  }

  const describe = (shown: Step) => {
    switch (shown.kind) {
      case 'series':
        return t('booharu.context.chooseSeriesHint')
      case 'separate':
        return t(separately(shown.why), { model: modelName(models, shown.model) })
      case 'pick':
        return `${t('booharu.context.pickReader')} ${t('booharu.context.readerDownload')}`
      case 'confirm':
        return t(shown.local ? 'booharu.context.localConfirm' : 'booharu.context.cloudConfirm', {
          model: modelName(models, shown.model),
        })
    }
  }

  const reads = reading || going
  const open = step !== null && pressedIn === project
  return (
    <>
      <Button
        type='button'
        size='sm'
        className='w-full text-[11px]'
        disabled={project === null || running || reads || busy}
        aria-busy={reads || busy}
        onClick={press}
      >
        {/* The desktop may take a while to say which model reads, listing every vendor's models. */}
        {reads || busy ? <LoaderCircle className='animate-spin' /> : <BookOpenText />}
        {reads ? t('booharu.context.reading') : t('booharu.context.readChapter')}
      </Button>
      <Dialog
        open={open}
        onOpenChange={(opened) => {
          if (!opened && !busy) close()
        }}
      >
        {step && (
          <DialogContent showCloseButton={false} className='gap-3 sm:max-w-md'>
            <DialogTitle className='text-[13px] font-semibold'>
              {t('booharu.context.readChapter')}
            </DialogTitle>
            <DialogDescription className='text-[11px]'>{describe(step)}</DialogDescription>
            {step.kind === 'series' && (
              <SeriesPicker
                value={null}
                disabled={busy}
                onChange={(id) => linkSeries(id).then(() => next(pressedIn))}
              />
            )}
            {step.kind === 'pick' && (
              <div className='rounded-lg border border-border/60 p-0.5'>
                <ReadingModelPicker
                  hint={false}
                  onPicked={(model) => confirm(pressedIn, model, isLocal(model), step.ask)}
                />
              </div>
            )}
            {step.kind === 'confirm' && (
              <label className='flex items-center gap-2 text-[12px]'>
                <Checkbox
                  checked={dontAsk}
                  disabled={busy}
                  onCheckedChange={(checked) => setDontAsk(checked)}
                />
                {t('booharu.context.dontAsk')}
              </label>
            )}
            <DialogFooter>
              <Button type='button' variant='ghost' disabled={busy} onClick={close}>
                {step.kind === 'series' || step.kind === 'pick'
                  ? t('common.cancel')
                  : t('booharu.context.no')}
              </Button>
              {step.kind === 'separate' && (
                <Button type='button' onClick={() => setStep({ kind: 'pick', ask: step.ask })}>
                  {t('booharu.context.yes')}
                </Button>
              )}
              {step.kind === 'confirm' && (
                <Button type='button' disabled={busy} aria-busy={busy} onClick={yes}>
                  {t('booharu.context.yes')}
                </Button>
              )}
            </DialogFooter>
          </DialogContent>
        )}
      </Dialog>
    </>
  )
}
