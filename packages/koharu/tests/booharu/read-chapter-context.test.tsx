import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { ContextPanel } from '@/components/booharu/context/ContextPanel'
import { useBooharuJobs } from '@/lib/booharu/booharuJobs'
import { refreshContext } from '@/lib/booharu/context'
import { cantRead, isLocal, type Reading } from '@/lib/booharu/reading'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type ChapterBrief,
  type ReadingModel,
  type ReadSummary,
} from '@koharu/ipc/protocol'
import { Toaster } from '@koharu/ui/components/toast'

import { blueLock, contextDesktop, openChapter } from './context'
import {
  gemma,
  gpt,
  indexTranslate,
  koharuPreferences,
  layOutModelMenus,
  models,
  textOnly,
} from './reading'
import { builtInReading, settingsDesktop } from './settings'

type User = ReturnType<typeof userEvent.setup>

/** What a read of 12 pages with Gemma did, `fields` over it. */
function readSummary(fields: Partial<ReadSummary> = {}): ReadSummary {
  return {
    model: gemma.reference,
    pages: 12,
    chunks: 1,
    summary_kept: false,
    characters: 3,
    suggestions_added: 2,
    lines_labelled: 40,
    notes_written: 5,
    kept_user_labels: 0,
    skipped_locked: 1,
    skipped_changed: 0,
    unknown_ids: 0,
    blank_terms: 0,
    stopped: false,
    ...fields,
  }
}

/** The desktop as `contextDesktop` has it, the chapter linking `linked` with `brief`, translating
 * with `translation`, and the Booharu settings file holding `reading`. The models a read would
 * use follow the settings as the desktop picks them; a read answers `summary`; a brief saved keeps
 * its characters and when and with what it was read. */
function readDesktop({
  linked = blueLock().id as string | null,
  translation = gpt.reference,
  reading = {} as Partial<Reading>,
  brief = null as ChapterBrief | null,
  summary = readSummary(),
} = {}) {
  const settings = settingsDesktop()
  settings.file.preferences['booharu-reading'] = { ...builtInReading, ...reading }
  const context = contextDesktop({ linked })
  // contextDesktop answers every command it doesn't serve with null, the settings' too.
  vi.spyOn(commands, 'booharuGetPreferences').mockImplementation(async () =>
    structuredClone(settings.file.preferences),
  )
  vi.spyOn(commands, 'booharuSavePreferences').mockImplementation(async (preferences) => {
    settings.saves.push(structuredClone(preferences))
    settings.file.preferences = structuredClone(preferences)
    return structuredClone(preferences)
  })

  const files = { brief }
  const chapter = context.spies.chapter.getMockImplementation()!
  vi.spyOn(commands, 'booharuGetChapterContext').mockImplementation(async () => {
    const state = await chapter()
    return { ...state, context: { ...state.context, brief: structuredClone(files.brief) } }
  })
  const setBrief = vi.spyOn(commands, 'booharuSetChapterBrief').mockImplementation(async (text) => {
    files.brief = {
      characters: [],
      read_at: null,
      read_model: null,
      ...files.brief,
      summary: text.trim(),
      edited: true,
    }
    return commands.booharuGetChapterContext()
  })

  const preferences = koharuPreferences(translation)
  vi.spyOn(commands, 'listModels').mockResolvedValue(models)
  vi.spyOn(commands, 'getPreferences').mockResolvedValue(preferences)
  useKoharuStore.setState({ models, preferences })
  layOutModelMenus()
  vi.spyOn(commands, 'booharuGetReadingModel').mockImplementation(
    async (): Promise<ReadingModel> => {
      const saved = settings.file.preferences['booharu-reading'].model ?? null
      const savedCantRead = saved ? cantRead(models, saved) : null
      const translationCantRead = cantRead(models, translation)
      const model = saved
        ? savedCantRead
          ? null
          : saved
        : translationCantRead
          ? null
          : translation
      return {
        model,
        local: model !== null && isLocal(model),
        saved,
        saved_cant_read: savedCantRead,
        translation,
        translation_cant_read: translationCantRead,
        explain: null,
      }
    },
  )
  const read = vi.spyOn(commands, 'booharuReadChapter').mockResolvedValue(summary)
  return { file: settings.file, saves: settings.saves, spies: context.spies, files, setBrief, read }
}

/** The Context tab on Chapter 1, with the app's toasts. */
async function renderPanel() {
  openChapter()
  render(
    <QueryClientProvider client={queryClient}>
      <ContextPanel />
      <Toaster />
    </QueryClientProvider>,
  )
  await waitFor(() => expect(readButton()).toBeEnabled())
}

const readButton = () => screen.getByRole('button', { name: 'Read chapter' })

/** The Read chapter dialog, none while it's closed. Toasts are dialogs too. */
const readDialog = () => screen.queryByRole('dialog', { name: 'Read chapter' })

/** The Read chapter dialog, once it asks `question`. */
async function asking(question: string | RegExp) {
  const dialog = await screen.findByRole('dialog', { name: 'Read chapter' })
  await waitFor(() => expect(dialog).toHaveAccessibleDescription(question))
  return within(dialog)
}

/** Answers the dialog with its button `name`. */
async function answer(user: User, dialog: ReturnType<typeof within>, name: string) {
  await user.click(dialog.getByRole('button', { name }))
}

beforeEach(() => {
  useBooharuJobs.setState({ jobs: {} })
})

describe('Read chapter in the Context tab', () => {
  it('links a series first, asks for a separate model, saves the one picked, and asks once more', async () => {
    const { read, saves } = readDesktop({
      linked: null,
      translation: textOnly.reference,
    })
    let finish!: (summary: ReadSummary) => void
    read.mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    const user = userEvent.setup()
    await renderPanel()

    await user.click(readButton())
    const series = await asking('Pick or create a series to keep the glossary across chapters.')
    await user.click(series.getByRole('button', { name: 'New series…' }))
    await user.type(series.getByRole('textbox', { name: 'Series name' }), 'Kagurabachi{Enter}')

    // The translation model sees no images, so no model can read the pages.
    const separate = await asking(
      "Text Only has no vision capabilities, so it can't see the pages. Use a separate model for reading?",
    )
    expect(commands.booharuLinkSeries).toHaveBeenCalledWith('new-series-000000000000')
    await answer(user, separate, 'Yes')

    const pick = await asking(
      'Choose a vision model for reading chapters. This choice is saved for next time. Local models download automatically when reading starts if needed.',
    )
    const choices = within(await pick.findByRole('listbox', { name: 'Choose a model' }))
    // Only the models that can read the pages.
    expect(choices.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'GPT-5.4',
      'Gemma 4 12B',
    ])
    await user.click(choices.getByRole('option', { name: 'Use Gemma 4 12B from Koharu' }))

    const confirm = await asking(
      'Gemma 4 12B will read the whole chapter on this computer. This can take a while. Are you sure?',
    )
    expect(saves).toHaveLength(1)
    expect(saves[0]!['booharu-reading']).toEqual({ ...builtInReading, model: gemma.reference })
    expect(read).not.toHaveBeenCalled()
    await user.click(confirm.getByRole('checkbox', { name: "Don't ask again" }))
    await answer(user, confirm, 'Yes')

    await waitFor(() => expect(read).toHaveBeenCalledWith(null))
    expect(saves).toHaveLength(2)
    expect(saves[1]!['booharu-reading']).toEqual({
      model: gemma.reference,
      ask: false,
      pages_per_request: 20,
    })
    expect(readDialog()).toBeNull()
    // The button says so while the chapter is read.
    expect(screen.getByRole('button', { name: 'Reading chapter…' })).toBeDisabled()

    await act(async () => finish(readSummary()))
    expect(await screen.findByText('Read 12 pages with Gemma 4 12B')).toBeInTheDocument()
    expect(
      screen.getByText(
        'Found 3 characters. Suggested 2 terms for the glossary. Set the lettering category of 40 lines. Wrote notes on 5 lines. Skipped 1 locked line.',
      ),
    ).toBeInTheDocument()
    await waitFor(() => expect(readButton()).toBeEnabled())
    expect(read).toHaveBeenCalledTimes(1)
  })

  it('says a translation-only model can’t read, and stops on No or Cancel', async () => {
    const { read, saves } = readDesktop({ translation: indexTranslate.reference })
    const user = userEvent.setup()
    await renderPanel()

    await user.click(readButton())
    const separate = await asking(
      "Index-Translate 9B only translates, so it can't read the chapter. Use a separate model for reading?",
    )
    await answer(user, separate, 'No')
    await waitFor(() => expect(readDialog()).toBeNull())

    await user.click(readButton())
    await answer(user, await asking(/only translates/), 'Yes')
    const pick = await asking(
      'Choose a vision model for reading chapters. This choice is saved for next time. Local models download automatically when reading starts if needed.',
    )
    await answer(user, pick, 'Cancel')
    await waitFor(() => expect(readDialog()).toBeNull())
    expect(saves).toEqual([])
    expect(read).not.toHaveBeenCalled()
  })

  it('asks before a hosted model reads, and reads at once with asking off', async () => {
    const { read, file, saves } = readDesktop({ reading: { model: gpt.reference } })
    const user = userEvent.setup()
    await renderPanel()

    await user.click(readButton())
    const confirm = await asking(
      'GPT-5.4 will read the whole chapter. This will use API credits. Are you sure?',
    )
    await answer(user, confirm, 'No')
    await waitFor(() => expect(readDialog()).toBeNull())
    expect(read).not.toHaveBeenCalled()

    // The agent's settings tool turns asking off meanwhile.
    file.preferences['booharu-reading'].ask = false
    read.mockResolvedValueOnce(readSummary({ model: gpt.reference, stopped: true, pages: 0 }))
    await user.click(readButton())
    await waitFor(() => expect(read).toHaveBeenCalledWith(null))
    expect(readDialog()).toBeNull()
    expect(
      await screen.findByText('Stopped reading the chapter. Nothing was saved.'),
    ).toBeInTheDocument()
    expect(saves).toEqual([])
  })

  it('shows a refusal’s error, and says Reading chapter… while a Run press reads', async () => {
    const { read } = readDesktop({ reading: { ask: false } })
    read.mockRejectedValueOnce("There's no text to read; run OCR first.")
    const user = userEvent.setup()
    await renderPanel()

    await user.click(readButton())
    expect(await screen.findByText("There's no text to read; run OCR first.")).toBeInTheDocument()
    await waitFor(() => expect(readButton()).toBeEnabled())

    act(() =>
      useBooharuJobs.setState({
        jobs: {
          4: {
            id: 4,
            kind: 'read_chapter',
            state: 'running',
            message: null,
            skip: null,
            step: 1,
            steps: 3,
            group: 2,
            summary: null,
          },
        },
      }),
    )
    expect(screen.getByRole('button', { name: 'Reading chapter…' })).toBeDisabled()
  })
})

describe('The chapter brief in the Context tab', () => {
  it('says when the chapter was last read and with what, and saves the summary written in View', async () => {
    const { files, setBrief } = readDesktop()
    const user = userEvent.setup()
    await renderPanel()
    expect(screen.getByRole('heading', { name: 'Chapter brief' })).toBeInTheDocument()
    expect(screen.getByText('Not read yet')).toBeInTheDocument()

    files.brief = {
      summary: 'Isagi joins Blue Lock.',
      characters: [
        { name: 'Isagi', original: '潔', pages: [5, 1, 2, 3] },
        { name: 'Bachira', original: 'Bachira', pages: [4] },
      ],
      edited: false,
      read_at: '2026-11-19T09:30:00Z',
      read_model: gpt.reference,
    }
    await act(() => refreshContext())
    const time = new Intl.DateTimeFormat('en-US', {
      dateStyle: 'medium',
      timeStyle: 'short',
    }).format(new Date('2026-11-19T09:30:00Z'))
    expect(await screen.findByText(`Last read ${time} with GPT-5.4`)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'View' }))
    const dialog = within(await screen.findByRole('dialog', { name: 'Chapter brief' }))
    const summary = dialog.getByRole('textbox', { name: 'Summary' })
    expect(summary).toHaveValue('Isagi joins Blue Lock.')
    expect(
      within(dialog.getByRole('list', { name: 'Characters' }))
        .getAllByRole('listitem')
        .map((item) => item.textContent),
    ).toEqual(['Isagi潔· Pages 1–3, 5', 'Bachira· Page 4'])
    const save = dialog.getByRole('button', { name: 'Save brief' })
    expect(save).toBeDisabled()

    await user.type(summary, ' He meets Bachira.  ')
    await user.click(save)
    await waitFor(() =>
      expect(setBrief).toHaveBeenCalledWith('Isagi joins Blue Lock. He meets Bachira.  '),
    )
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Chapter brief' })).toBeNull())
    expect(files.brief).toMatchObject({
      summary: 'Isagi joins Blue Lock. He meets Bachira.',
      edited: true,
    })
    // Saving keeps when and with what the chapter was read.
    expect(screen.getByText(`Last read ${time} with GPT-5.4`)).toBeInTheDocument()

    files.brief = { ...files.brief!, read_model: null }
    await act(() => refreshContext())
    expect(await screen.findByText(`Last read ${time} with an earlier model`)).toBeInTheDocument()
  })
})
