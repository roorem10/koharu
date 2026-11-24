import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { CanvasCommandBar } from '@/components/editor/CanvasCommandBar'
import { SettingsPage } from '@/components/preferences/SettingsPage'
import { useReadSwitches } from '@/lib/booharu/reading'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type RunPlan } from '@koharu/ipc/protocol'
import { Toaster } from '@koharu/ui/components/toast'

import { every, openChapter } from './jobs'
import { chapterState, gpt, indexTranslate, readingDesktop } from './reading'
import { builtInRuns } from './settings'

type User = ReturnType<typeof userEvent.setup>

/** The editor's command bar on Chapter 1, booharu_run answering `plan`. */
function renderCommandBar(plan: RunPlan = { group: 1, groups: [], run: 3, reading: false }) {
  const carryOn = vi.spyOn(commands, 'booharuRun').mockResolvedValue(plan)
  const run = vi.spyOn(commands, 'runPipeline').mockResolvedValue(4)
  openChapter('Chapter 1')
  render(
    <QueryClientProvider client={queryClient}>
      <CanvasCommandBar />
      <Toaster />
    </QueryClientProvider>,
  )
  return { carryOn, run }
}

/** Opens the Run menu on its stages. */
async function openStages(user: User) {
  await user.click(screen.getByRole('button', { name: 'Processing settings' }))
  await user.click(await screen.findByRole('button', { name: /^Stages / }))
}

const readRow = () => screen.getByRole('button', { name: /^Analyze chapter/ })

/** The Run menu's stages, as the stage list and the preset editor show them, in order. */
const stageNames = ['Detection', 'OCR', 'Inpainting', 'Analyze chapter', 'Translation']

/** The Run menu's presets. */
const presets = () => within(screen.getByRole('group', { name: 'Presets' }))

beforeEach(() => {
  useReadSwitches.setState({ on: {} })
})

describe('Booharu’s Analyze chapter in the Run menu', () => {
  it('starts on with a series, no brief and a model to read it, before Translation', async () => {
    readingDesktop()
    const user = userEvent.setup()
    renderCommandBar()
    await openStages(user)

    await waitFor(() => expect(readRow()).toHaveAttribute('aria-pressed', 'true'))
    expect(readRow()).toHaveTextContent(
      'Analyze chapterAnalyzes the whole chapter with GPT-5.4 before translating.',
    )
    const order = screen
      .getAllByRole('button')
      .flatMap((button) => stageNames.filter((name) => button.textContent?.startsWith(name)))
    expect(order).toEqual(stageNames)

    // Off while Translation is, and saying why.
    await user.click(screen.getByRole('button', { name: /^Translation/ }))
    expect(readRow()).toBeDisabled()
    expect(readRow()).toHaveAttribute('aria-pressed', 'false')
    expect(readRow()).toHaveTextContent('Turn on Translation to analyze the chapter first.')
  })

  it('wraps its line, so a long one never widens the stage list past the menu', async () => {
    readingDesktop()
    const user = userEvent.setup()
    renderCommandBar()
    await openStages(user)
    await waitFor(() =>
      expect(readRow()).toHaveTextContent(
        'Analyzes the whole chapter with GPT-5.4 before translating.',
      ),
    )

    // On one line, as a Button keeps its text, this row would size the stage list's one column
    // past the menu's width, and the menu's edge would cut off every stage's check mark.
    expect(readRow()).toHaveClass('whitespace-normal')
    expect(readRow()).not.toHaveClass('whitespace-nowrap')
  })

  it('starts off once the chapter has a brief', async () => {
    readingDesktop({ chapter: chapterState({ summary: 'Isagi meets the team.' }) })
    const user = userEvent.setup()
    renderCommandBar()
    await openStages(user)
    await waitFor(() =>
      expect(readRow()).toHaveTextContent(
        'Analyzes the whole chapter with GPT-5.4 before translating.',
      ),
    )
    expect(readRow()).toHaveAttribute('aria-pressed', 'false')
  })

  it('opens the reading-model picker when switched on with no model, and stays off on Cancel', async () => {
    const { saves } = readingDesktop({ translation: indexTranslate.reference })
    const user = userEvent.setup()
    renderCommandBar()
    await openStages(user)
    await waitFor(() =>
      expect(readRow()).toHaveTextContent('Choose a model that can analyze the pages.'),
    )
    expect(readRow()).toHaveAttribute('aria-pressed', 'false')

    await user.click(readRow())
    const picker = within(await screen.findByRole('listbox', { name: 'Choose a model' }))
    // Only the models that can read the pages.
    expect(picker.getAllByRole('option').map((option) => option.textContent)).toEqual([
      'GPT-5.4',
      'Gemma 4 12B',
    ])
    expect(
      screen.getByText(/Choose a vision model for analyzing chapters\. This choice is saved/),
    ).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(readRow()).toHaveAttribute('aria-pressed', 'false')
    expect(saves).toEqual([])

    await user.click(readRow())
    await user.click(await screen.findByRole('option', { name: 'Use GPT-5.4 from OpenAI' }))
    await waitFor(() => expect(readRow()).toHaveAttribute('aria-pressed', 'true'))
    expect(saves.at(-1)?.['booharu-reading']).toEqual({
      model: gpt.reference,
      ask: true,
      pages_per_request: 20,
    })
    expect(readRow()).toHaveTextContent(
      'Analyzes the whole chapter with GPT-5.4 before translating.',
    )
  })
})

describe('A Run press with Analyze chapter', () => {
  it('sends Read to booharu_run, also with only Translation on', async () => {
    readingDesktop()
    const user = userEvent.setup()
    const { carryOn, run } = renderCommandBar({
      group: 1,
      groups: [{ stages: [], pages: ['p1', 'p2', 'p3', 'p4'], read: true }],
      run: null,
      reading: true,
    })

    await user.click(screen.getByRole('button', { name: 'Run processing' }))
    await waitFor(() =>
      expect(carryOn).toHaveBeenCalledWith({ kind: 'project' }, [
        'detect',
        'recognize',
        'inpaint',
        'read',
        'translate',
      ]),
    )
    // The group reads first, so no run goes yet: the pages aren't done.
    await act(async () => undefined)
    expect(screen.queryByText('These pages are already done.')).toBeNull()

    act(() => useKoharuStore.setState({ processingStages: ['translate'] }))
    await user.click(screen.getByRole('button', { name: 'Run processing' }))
    await waitFor(() =>
      expect(carryOn).toHaveBeenLastCalledWith({ kind: 'project' }, ['read', 'translate']),
    )
    expect(run).not.toHaveBeenCalled()

    // Switched off, only Translation runs Koharu's way.
    act(() => useReadSwitches.setState({ on: { 'Chapter 1': false } }))
    await user.click(screen.getByRole('button', { name: 'Run processing' }))
    await waitFor(() => expect(run).toHaveBeenCalledWith('project', ['translate']))
    expect(carryOn).toHaveBeenCalledTimes(2)
  })

  it('says the pages are done only when the plan has no group left', async () => {
    readingDesktop({ chapter: chapterState({ summary: 'Read already.' }) })
    const user = userEvent.setup()
    const { carryOn } = renderCommandBar({ group: 2, groups: [], run: null, reading: false })

    await user.click(screen.getByRole('button', { name: 'Run processing' }))
    expect(await screen.findByText('These pages are already done.')).toBeInTheDocument()
    expect(carryOn).toHaveBeenCalledWith({ kind: 'project' }, every)
  })
})

describe('Booharu presets with Analyze chapter in the Run menu', () => {
  it('sets Analyze chapter from a preset, matches it, and saves it with the stages', async () => {
    const { file, saves } = readingDesktop({ chapter: chapterState({ summary: 'A match.' }) })
    file.preferences['booharu-runs'].presets = [
      ...builtInRuns.presets,
      { name: 'Read + translate', stages: ['recognize', 'read', 'translate'] },
      { name: 'OCR + translate', stages: ['recognize', 'translate'] },
    ]
    const user = userEvent.setup()
    renderCommandBar()
    await user.click(screen.getByRole('button', { name: 'Processing settings' }))
    const reads = await presets().findByRole('button', { name: 'Read + translate' })
    const translates = presets().getByRole('button', { name: 'OCR + translate' })

    await user.click(reads)
    expect(useKoharuStore.getState().processingStages).toEqual(['recognize', 'translate'])
    expect(useReadSwitches.getState().on).toEqual({ 'Chapter 1': true })
    // A preset matches only with Analyze chapter as it has it.
    expect(reads).toHaveAttribute('aria-pressed', 'true')
    expect(translates).toHaveAttribute('aria-pressed', 'false')

    await user.click(translates)
    expect(useReadSwitches.getState().on).toEqual({ 'Chapter 1': false })
    expect(reads).toHaveAttribute('aria-pressed', 'false')
    expect(translates).toHaveAttribute('aria-pressed', 'true')

    await user.click(screen.getByRole('button', { name: /^Stages / }))
    await user.click(readRow())
    expect(readRow()).toHaveAttribute('aria-pressed', 'true')
    await user.click(screen.getByRole('button', { name: 'Save as preset' }))
    await user.type(screen.getByRole('textbox', { name: 'Preset name' }), 'Mine{Enter}')
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!['booharu-runs'].presets?.at(-1)).toEqual({
      name: 'Mine',
      stages: ['recognize', 'read', 'translate'],
    })
  })
})

describe('Booharu Runs settings with Analyze chapter', () => {
  it('ticks Analyze chapter between Inpainting and Translation, only with Translation', async () => {
    const { saves } = readingDesktop()
    useKoharuStore.setState({ settingsOpen: true })
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )
    await user.click(screen.getByRole('tab', { name: 'Runs' }))
    const detect = within(await screen.findByRole('group', { name: 'Detect + OCR' }))
    const translate = within(screen.getByRole('group', { name: 'Translate + clean up' }))

    expect(translate.getAllByRole('checkbox')).toEqual(
      stageNames.map((name) => translate.getByRole('checkbox', { name })),
    )
    expect(detect.getByRole('checkbox', { name: 'Analyze chapter' })).toHaveAttribute(
      'aria-disabled',
      'true',
    )

    await user.click(translate.getByRole('checkbox', { name: 'Analyze chapter' }))
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!['booharu-runs'].presets?.[1]).toEqual({
      name: 'Translate + clean up',
      stages: ['inpaint', 'read', 'translate'],
    })
    // The preset is the user's own now, so it shows its name as saved.
    const mine = within(await screen.findByRole('group', { name: 'Translate + clean up' }))
    await user.click(mine.getByRole('checkbox', { name: 'Translation' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(saves[1]!['booharu-runs'].presets?.[1]?.stages).toEqual(['inpaint'])
  })
})
