import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import type { ReactNode } from 'react'
import { expect, vi } from 'vitest'

import { queryClient } from '@/lib/queries'
import {
  commands,
  type BlockLettering,
  type BlockShape,
  type BooharuPreferences,
  type CategoryCounts,
} from '@koharu/ipc/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

import { boxesPage, desktop, openProject } from './boxes'
import { builtInLettering, builtInRuns } from './settings'

/** A balloon's shape with the measures that decided it. */
function shape(
  kind: BlockShape['shape'],
  rectangularity: number,
  solidity: number,
  spikes: number,
) {
  return { shape: kind, measures: { rectangularity, solidity, spikes } }
}

const round = shape('round', 0.934, 0.951, 0)

/** The boxes page's blocks as the desktop letters them: "Hel" by its round balloon, "lo" a shout
 * the user picked, "Bang" Koharu's sound effect, "Hi" a whisper the agent picked, "Free" in no
 * balloon, and the locked blocks small text and by their balloon. */
export const lettering: BlockLettering[] = [
  { block: 'first', category: 'normal_dialogue', reason: 'shape', label: null, shape: round },
  {
    block: 'second',
    category: 'shout',
    reason: 'user',
    label: { category: 'shout', source: 'user' },
    shape: round,
  },
  { block: 'bang', category: 'sfx', reason: 'sfx', label: null, shape: round },
  {
    block: 'alone',
    category: 'whisper',
    reason: 'model',
    label: { category: 'whisper', source: 'model', confidence: 0.9 },
    shape: shape('spiky', 0.6, 0.7, 7),
  },
  { block: 'held-first', category: 'narration_box', reason: 'shape', label: null, shape: null },
  { block: 'held-second', category: 'narration_box', reason: 'shape', label: null, shape: null },
  { block: 'free', category: 'floating', reason: 'floating', label: null, shape: null },
  { block: 'locked', category: 'small_text', reason: 'small_text', label: null, shape: null },
].map((entry) => ({ hand_edited: false, ...entry }) as BlockLettering)

/** Every lettering category's name, in Settings' order. */
export const categoryNames = [
  'Normal Dialogue',
  'Thoughts',
  'Floating',
  'Small Text',
  'Narration Box',
  'Shout',
  'Weak/Panic Shout',
  'SFX',
  'Moan',
  'Title',
  'Phone Text',
  'Whisper',
  'Shy / Hesitant',
  'Crying / Trembling',
  'Menacing / Sinister',
  'Handwritten',
  'Sign or printed text',
  'Electronic voice',
]

/** A category change of one block, as the desktop answers it. */
export const counts: CategoryCounts = {
  changed: 1,
  locked: 0,
  kept_user_label: 0,
  kept_hand_edited: 0,
  untranslated: 0,
}

/** The hint a picker shows while Pick fonts automatically is off, naming the Settings tab as
 * en-US labels it. */
const autoOff = 'Fonts follow categories only while Pick fonts automatically is on (Renderer).'

/** Matches the hint's line, whose link splits its text. */
export function hint(_: string, element: Element | null) {
  return element?.tagName === 'P' && element.textContent === autoOff
}

/** The desktop with the boxes page open, its blocks lettered as above, Pick fonts automatically
 * `auto`, and each category change answered with `counts`. */
export function letteredDesktop(auto = false) {
  const mocks = desktop()
  mocks.lettering.mockResolvedValue(lettering)
  const preferences: BooharuPreferences = {
    'booharu-mcp': { port: 47821 },
    'booharu-ocr-box': { default_mode: 'read' },
    'booharu-lettering': { ...builtInLettering, auto },
    'booharu-fonts': { favorites: [] },
    'booharu-runs': builtInRuns,
  }
  vi.spyOn(commands, 'booharuGetPreferences').mockResolvedValue(preferences)
  const setCategory = vi.spyOn(commands, 'booharuSetLetteringCategory').mockResolvedValue(counts)
  openProject(boxesPage())
  return { ...mocks, setCategory }
}

/** Renders `ui` as the app does, its tooltips showing at once. */
export function mount(ui: ReactNode) {
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delay={0}>{ui}</TooltipProvider>
    </QueryClientProvider>,
  )
}

/** The category picker's search line, once its list opens and takes the focus. */
export async function categorySearch() {
  const search = await screen.findByRole('combobox', { name: 'Search categories' })
  await waitFor(() => expect(search).toHaveFocus())
  return search
}

/** The choices the open picker lists, by name, in order. */
export function choicesListed() {
  return screen.queryAllByRole('option').map((option) => option.textContent)
}
