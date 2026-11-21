import { QueryClientProvider } from '@tanstack/react-query'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BoxDetailsDialog } from '@/components/booharu/details/BoxDetailsDialog'
import { RightSidebar } from '@/components/editor/RightSidebar'
import { StoryPanel } from '@/components/editor/StoryPanel'
import { useAgentChat } from '@/lib/booharu/agentChat'
import { boxActions, runBoxAction, selectedBoxes } from '@/lib/booharu/boxActions'
import { closeBoxDetails, openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { useExplanations } from '@/lib/booharu/explain'
import { resources, supportedLanguages } from '@/lib/i18n'
import { modelsKey, preferencesKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type ExplainAnswer, type Page, type ReadingModel } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { block, boxesPage, change, desktop, openProject } from './boxes'
import { gpt, koharuPreferences, models } from './reading'

function mount(ui: ReactNode) {
  return render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** The boxes page, its free block reading 潔の武器, translated "Isagi's weapon", with Read
 * chapter's note on it. */
function explainPage(translation = "Isagi's weapon"): Page {
  const page = boxesPage()
  page.layers = page.layers.map((layer) =>
    layer.id === 'free'
      ? block('free', '潔の武器', {
          translation: { content: [{ insert: translation }] },
          attributes: { booharu: { read_note: 'Isagi names the skill he wins with.' } },
        })
      : layer,
  )
  return page
}

/** What the model answers about the free block, as it reads now. */
const answer: ExplainAnswer = {
  layer: 'free',
  source: '潔の武器',
  translation: "Isagi's weapon",
  explanation: '武器 is his weapon: the skill he means to score with.',
  alternatives: ["Isagi's secret weapon", "Isagi's edge"],
  model: gpt.reference,
}

/** Which models Read chapter and Explain this line use, Explain this line's `explain`. */
function readingModel(explain: string | null): ReadingModel {
  return {
    model: explain,
    local: false,
    saved: null,
    saved_cant_read: null,
    translation: gpt.reference,
    translation_cant_read: null,
    explain,
  }
}

/** Every command answered without the desktop, Explain this line asking `explain`, which answers
 * `answer`; the boxes page open with the free block translated. */
function setUp(explain: string | null = gpt.reference) {
  const spies = desktop()
  queryClient.setQueryData(modelsKey, models)
  const reading = vi
    .spyOn(commands, 'booharuGetReadingModel')
    .mockResolvedValue(readingModel(explain))
  const ask = vi.spyOn(commands, 'booharuExplainLine').mockResolvedValue(answer)
  const edit = vi.spyOn(commands, 'edit').mockResolvedValue(change)
  const typed = vi.spyOn(commands, 'booharuSetTranslation').mockResolvedValue(change)
  openProject(explainPage())
  return { ...spies, reading, ask, edit, typed }
}

/** Holds back the desktop's answers to which models read and explain until `answer` is called,
 * which gives each one asked `explain` as Explain this line's model. */
function holdReadingModel(reading: ReturnType<typeof setUp>['reading']) {
  const held: ((model: ReadingModel) => void)[] = []
  reading.mockImplementation(() => new Promise((resolve) => held.push(resolve)))
  return {
    answer: (explain: string | null = gpt.reference) =>
      act(async () => {
        for (const tell of held.splice(0)) tell(readingModel(explain))
      }),
  }
}

/** Box details' Explain this line section, once it shows. */
async function explanation() {
  return within(await screen.findByRole('region', { name: 'Explanation' }))
}

/** The open project's lines, each explained with its answer, as Explain this line left them. */
function explained(...answers: ExplainAnswer[]) {
  useExplanations.setState({
    project: 'Demo',
    lines: Object.fromEntries(
      answers.map((line) => [line.layer, { state: 'answered', answer: line }]),
    ),
  })
}

/** Right-clicks Story's source of the free block, then picks Explain this line. */
async function explainFromStory(user: ReturnType<typeof userEvent.setup>) {
  const source = await screen.findByRole('textbox', { name: "Isagi's weapon source text" })
  fireEvent.contextMenu(source, { button: 2, clientX: 20, clientY: 30 })
  await user.click(await screen.findByRole('menuitem', { name: 'Explain this line' }))
}

beforeEach(() => {
  useBoxDetails.setState({ shown: null, reveal: null })
  useExplanations.setState({ project: null, lines: {} })
  useAgentChat.setState({ project: null, turns: [], running: null, stale: false })
})

describe('Booharu Explain this line', () => {
  it("explains a line from Story's right-click menu, and says which model answered", async () => {
    const { ask } = setUp()
    let answerLine: (value: ExplainAnswer) => void = () => undefined
    ask.mockImplementation(() => new Promise((resolve) => (answerLine = resolve)))
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    mount(
      <>
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    await explainFromStory(user)

    const card = await explanation()
    expect(useBoxDetails.getState().shown).toEqual({
      page: 'page',
      layer: 'free',
      section: 'explain',
    })
    expect(card.getByText('Isagi names the skill he wins with.')).toBeInTheDocument()
    // Told politely to a screen reader as it goes, then as it's answered.
    const told = within(card.getByRole('status'))
    expect(await told.findByRole('button', { name: 'Explaining…' })).toBeDisabled()
    expect(ask.mock.calls).toEqual([['free']])

    act(() => answerLine(answer))

    expect(await told.findByText(answer.explanation)).toBeInTheDocument()
    const list = card.getByRole('list', { name: 'Other translations' })
    expect(
      within(list)
        .getAllByRole('listitem')
        .map((item) => item.querySelector('p')?.textContent),
    ).toEqual(answer.alternatives)
    expect(card.getByText('Answered by GPT-5.4')).toBeInTheDocument()
    expect(card.queryByRole('button', { name: 'Explain this line' })).toBeNull()
    expect(shown).not.toHaveBeenCalled()
  })

  it('opens Box details at once, and asks once the desktop says which model explains lines', async () => {
    const { reading, ask } = setUp()
    const models = holdReadingModel(reading)
    const user = userEvent.setup()
    mount(
      <>
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    await explainFromStory(user)

    // Box details shows the line while the model is still unknown, Explain this line off.
    const card = await explanation()
    expect(card.getByRole('button', { name: 'Explain this line' })).toBeDisabled()
    expect(ask).not.toHaveBeenCalled()

    await models.answer()

    expect(await card.findByText(answer.explanation)).toBeInTheDocument()
    expect(ask.mock.calls).toEqual([['free']])
  })

  it('asks nothing for a line Box details no longer shows once the model is known', async () => {
    const { reading, ask } = setUp()
    const models = holdReadingModel(reading)
    mount(<BoxDetailsDialog />)

    const explain = boxActions.find(({ id }) => id === 'explain')!
    const boxes = selectedBoxes(explainPage().layers, ['free-mask'])
    act(() => runBoxAction(explain, { page: 'page', boxes }))
    await explanation()
    act(() => closeBoxDetails())
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())

    await models.answer()

    expect(ask).not.toHaveBeenCalled()
    expect(useExplanations.getState().lines).toEqual({})
  })

  it('makes another translation the line’s as one edit, and offers none for a locked line', async () => {
    const { edit, typed } = setUp()
    const untranslated = {
      ...answer,
      layer: 'alone',
      source: 'Hi',
      translation: '',
      alternatives: ['Hey there'],
    }
    const locked = { ...answer, layer: 'locked', source: 'Locked', translation: '' }
    explained(answer, untranslated, locked)
    const user = userEvent.setup()
    mount(<BoxDetailsDialog />)

    // A translated line: its translation's text is replaced.
    act(() => openBoxDetails('page', 'free', 'explain'))
    let card = await explanation()
    const [first] = card.getAllByRole('button', { name: 'Use this' })
    expect(first).toHaveAccessibleDescription("Isagi's secret weapon")
    await user.click(first!)
    await waitFor(() => expect(edit).toHaveBeenCalledTimes(1))
    expect(edit).toHaveBeenCalledWith({
      label: 'set_text',
      ops: [{ op: 'set_text', layer: 'free', text: "Isagi's secret weapon" }],
    })
    expect(typed).not.toHaveBeenCalled()

    // A line with no translation yet: its first one, placed in its frame.
    act(() => openBoxDetails('page', 'alone', 'explain'))
    card = await explanation()
    await user.click(await card.findByRole('button', { name: 'Use this' }))
    await waitFor(() => expect(typed).toHaveBeenCalledTimes(1))
    expect(typed).toHaveBeenCalledWith('alone', 'Hey there')
    expect(edit).toHaveBeenCalledTimes(1)

    // A locked line takes none, and the card says why.
    act(() => openBoxDetails('page', 'locked', 'explain'))
    card = await explanation()
    await waitFor(() =>
      expect(card.getAllByRole('button', { name: 'Use this' })).toHaveLength(
        locked.alternatives.length,
      ),
    )
    for (const button of card.getAllByRole('button', { name: 'Use this' })) {
      expect(button).toBeDisabled()
    }
    expect(
      card.getByText(
        "This text, or a layer it's in, is locked. Unlock it in Layers to use another translation.",
      ),
    ).toBeInTheDocument()
    expect(edit).toHaveBeenCalledTimes(1)
    expect(typed).toHaveBeenCalledTimes(1)
  })

  it('drops an answer once the line changed since it was asked', async () => {
    setUp()
    explained(answer)
    mount(<BoxDetailsDialog />)

    act(() => openBoxDetails('page', 'free', 'explain'))
    const card = await explanation()
    expect(card.getByText(answer.explanation)).toBeInTheDocument()

    // Another writer, or Use this, changes the translation.
    act(() => openProject(explainPage("Isagi's edge")))

    await waitFor(() => expect(card.queryByText(answer.explanation)).toBeNull())
    await waitFor(() =>
      expect(card.getByRole('button', { name: 'Explain this line' })).toBeEnabled(),
    )
  })

  it('is off, and says where to choose a model, while no model explains lines', async () => {
    const { ask } = setUp(null)
    const user = userEvent.setup()
    mount(
      <>
        <StoryPanel />
        <BoxDetailsDialog />
      </>,
    )

    await explainFromStory(user)

    const card = await explanation()
    const hint = await card.findByText(
      (_, element) =>
        element?.tagName === 'P' &&
        element.textContent ===
          'No model can explain lines. Choose a Read chapter model (Pipeline), or a model for the agent.',
    )
    expect(hint).toBeInTheDocument()
    expect(card.getByRole('button', { name: 'Explain this line' })).toBeDisabled()
    expect(ask).not.toHaveBeenCalled()

    await user.click(card.getByRole('button', { name: 'Read chapter model' }))
    expect(useKoharuStore.getState().settingsOpen).toBe(true)
  })

  it('says why an explanation failed, and asks again on Try again, from the box action', async () => {
    const { ask } = setUp()
    ask.mockRejectedValueOnce('Explain this line was stopped.')
    const user = userEvent.setup()
    mount(<BoxDetailsDialog />)

    const explain = boxActions.find(({ id }) => id === 'explain')!
    const boxes = selectedBoxes(explainPage().layers, ['free-mask'])
    act(() => runBoxAction(explain, { page: 'page', boxes }))

    const card = await explanation()
    expect(await card.findByRole('alert')).toHaveTextContent('Explain this line was stopped.')
    await user.click(card.getByRole('button', { name: 'Try again' }))

    expect(await card.findByText(answer.explanation)).toBeInTheDocument()
    expect(card.queryByRole('alert')).toBeNull()
    expect(ask.mock.calls).toEqual([['free'], ['free']])
  })

  it("takes the line to the agent's composer once, and shows the Agent tab", async () => {
    setUp()
    explained(answer)
    queryClient.setQueryData(preferencesKey, koharuPreferences(gpt.reference))
    useKoharuStore.setState({ panel: 'story' })
    const user = userEvent.setup()
    const view = mount(
      <>
        <RightSidebar />
        <BoxDetailsDialog />
      </>,
    )

    act(() => openBoxDetails('page', 'free', 'explain'))
    const card = await explanation()
    await user.click(card.getByRole('button', { name: 'Discuss' }))

    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(useKoharuStore.getState().panel).toBe('agent')
    expect(screen.getByRole('tab', { name: 'Agent' })).toHaveAttribute('aria-selected', 'true')
    const composer = screen.getByRole('textbox', { name: 'Message Booharu Agent' })
    expect(composer).toHaveValue(
      `Discuss text layer free. Source: 潔の武器. Translation: Isagi's weapon. Explanation: ${answer.explanation}. Use view_page for the page image.`,
    )

    // Nothing is sent, and the line goes in once: the chat's panel, mounted again, starts empty.
    expect(commands.runAgent).not.toHaveBeenCalled()
    view.unmount()
    mount(<RightSidebar />)
    expect(screen.getByRole('textbox', { name: 'Message Booharu Agent' })).toHaveValue('')
  })

  it('has its strings in all 12 languages, each filling in and linking what English does', () => {
    const keys = [
      'booharu.context.explain',
      'booharu.context.explaining',
      'booharu.context.useThis',
      'booharu.context.useThisLocked',
      'booharu.context.discuss',
      'booharu.context.discussPrompt',
      'booharu.context.answeredBy',
      'booharu.context.readNote',
      'booharu.context.alternatives',
      'booharu.context.explainNeedsModel',
      'booharu.details.sections.explain',
    ]
    const text = (language: string, key: string) =>
      key
        .split('.')
        .reduce<unknown>(
          (node, part) => (node as Record<string, unknown> | undefined)?.[part],
          resources[language].translation,
        )
    /** What a string fills in and links: its `{{values}}` and `<a>` tags. */
    const fills = (value: unknown) =>
      typeof value === 'string' ? (value.match(/{{\w+}}|<\/?a>/g) ?? []).sort() : null
    expect(supportedLanguages).toHaveLength(12)
    for (const language of supportedLanguages) {
      for (const key of keys) {
        const value = text(language, key)
        expect(typeof value === 'string' && value.trim() !== '', `${language}: ${key}`).toBe(true)
        expect(fills(value), `${language}: ${key}`).toEqual(fills(text('en-US', key)))
      }
    }
  })
})
