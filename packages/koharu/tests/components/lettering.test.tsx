import { QueryClientProvider } from '@tanstack/react-query'
import { render as testingRender, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { type ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import { LetteringCategoryLine } from '@/components/editor/LetteringCategoryLine'
import { SfxPreferences } from '@/components/preferences/SfxPreferences'
import { ShapeStylePreferences } from '@/components/preferences/ShapeStylePreferences'
import { fontsKey, projectKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  emptyTextStyle,
  emptyShapeStyles,
  emptyCategoryStyles,
  suggestCategoryFonts,
} from '@/lib/text-styles'
import {
  commands,
  type FontFamily,
  type TextStyles,
  type LineCategory,
} from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

function render(children: ReactNode) {
  queryClient.setQueryData(projectKey, {
    name: 'chapter',
    revision: 1,
    active_page: null,
    can_undo: false,
    can_redo: false,
  })
  return testingRender(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>{children}</TooltipProvider>
    </QueryClientProvider>,
  )
}

function styles() {
  return {
    default_style: { ...emptyTextStyle },
    shape_styles: structuredClone(emptyShapeStyles),
    category_styles: structuredClone(emptyCategoryStyles),
    legacy_color_migrated: true,
  } satisfies TextStyles
}

function family(name: string): FontFamily {
  return {
    name,
    local_names: [],
    sources: ['system'],
    metadata: {
      primary_script: 'latn',
      scripts: [],
      languages: [],
      category: null,
      classifications: [],
      use_cases: [],
    },
    faces: [],
  }
}

describe('Inspector lettering category', () => {
  it.each([
    ['user', 'set by hand'],
    ['model', 'from chapter read'],
    ['small_text', 'small original lettering'],
    ['bubble_shape', 'rectangle bubble'],
  ] as const)('shows the effective category with reason %s', async (reason, text) => {
    vi.spyOn(commands, 'getEffectiveCategory').mockResolvedValue({ category: 'whisper', reason })
    vi.spyOn(commands, 'getTextBubbleShape').mockResolvedValue({
      bubble: 'bubble',
      shape: 'rectangle',
      measures: null,
    })
    render(<LetteringCategoryLine layer='line' />)
    expect(await screen.findByText(text)).toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Lettering category' })).toHaveTextContent(
      'Whisper',
    )
  })

  it('sets a hand category, then clears it with Automatic and refetches the effective result', async () => {
    let saved: LineCategory | null = null
    vi.mocked(commands.getLineCategory).mockImplementation(async () => saved)
    vi.mocked(commands.getEffectiveCategory).mockImplementation(async () => ({
      category: saved?.value ?? 'floating',
      reason: saved ? 'user' : 'bubble_shape',
    }))
    const set = vi
      .spyOn(commands, 'setLineCategory')
      .mockImplementation(async (_layer, category) => {
        saved = category ? { value: category, source: 'user', confidence: null } : null
        return saved
      })
    render(<LetteringCategoryLine layer='line' />)
    const user = userEvent.setup()
    const picker = screen.getByRole('combobox', { name: 'Lettering category' })
    await waitFor(() => expect(picker).toBeEnabled())
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: 'Thoughts' }))
    await waitFor(() => expect(set).toHaveBeenLastCalledWith('line', 'thoughts'))
    await screen.findByText('set by hand')
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: 'Automatic' }))
    await waitFor(() => expect(set).toHaveBeenLastCalledWith('line', null))
    await screen.findByText('no bubble')
    expect(picker).toHaveTextContent('Floating')
  })
})

describe('Lettering font preferences', () => {
  it('renders all 15 rows and preserves the round font when toggling Thoughts italic', async () => {
    const original = styles()
    original.shape_styles.round.font = {
      preferred_font: 'Dialogue',
      font_style: null,
      font_weight: 400,
    }
    original.category_styles.thoughts.fill = [1, 2, 3, 255]
    useKoharuStore.setState({ textStyles: original })
    queryClient.setQueryData(fontsKey, [])
    const save = vi.spyOn(commands, 'saveTextStyles').mockImplementation(async (styles) => styles)
    render(<ShapeStylePreferences />)
    expect(screen.getAllByTestId(/^lettering-row-/)).toHaveLength(15)
    expect(screen.getAllByRole('checkbox')).toHaveLength(30)
    const user = userEvent.setup()
    await user.click(screen.getByRole('checkbox', { name: 'Thoughts: Italic' }))
    await waitFor(() =>
      expect(save).toHaveBeenLastCalledWith({
        ...original,
        category_styles: {
          ...original.category_styles,
          thoughts: {
            ...original.category_styles.thoughts,
            font: { preferred_font: 'Dialogue', font_style: 'italic', font_weight: 400 },
          },
        },
      }),
    )
    await user.click(screen.getByRole('checkbox', { name: 'Thoughts: Bold' }))
    await waitFor(() =>
      expect(save.mock.lastCall?.[0].category_styles?.thoughts?.font).toEqual({
        preferred_font: 'Dialogue',
        font_style: 'italic',
        font_weight: 700,
      }),
    )
  })

  it('suggests only available fonts and leaves every filled style intact', async () => {
    const original = styles()
    original.shape_styles.round.font = {
      preferred_font: 'My Dialogue',
      font_style: null,
      font_weight: null,
    }
    original.shape_styles.rectangle.fill = [1, 2, 3, 255]
    useKoharuStore.setState({ textStyles: original })
    queryClient.setQueryData(fontsKey, [
      family('CCWildWords'),
      family('ANIMEACE2.0BB'),
      family('Darumadrop One'),
      family('CCTallTales'),
    ])
    const save = vi.spyOn(commands, 'saveTextStyles').mockImplementation(async (styles) => styles)
    render(<ShapeStylePreferences />)
    const user = userEvent.setup()
    await user.click(screen.getByRole('button', { name: 'Suggest fonts' }))
    await waitFor(() => expect(save).toHaveBeenCalledOnce())
    const saved = save.mock.lastCall![0]
    expect(saved.shape_styles).toEqual(original.shape_styles)
    expect(saved.category_styles?.thoughts?.font).toEqual({
      preferred_font: 'ANIMEACE2.0BB',
      font_style: 'italic',
      font_weight: null,
    })
    expect(saved.category_styles?.sfx?.font?.preferred_font).toBe('Darumadrop One')
    expect(saved.category_styles?.title).toEqual(emptyTextStyle)
    expect(
      suggestCategoryFonts(saved, [
        family('CCWildWords'),
        family('ANIMEACE2.0BB'),
        family('Darumadrop One'),
      ]),
    ).toBe(saved)
    expect(suggestCategoryFonts(original, [])).toBe(original)
  })
})

describe('SFX preferences', () => {
  it('saves the selected mode without changing other lettering settings', async () => {
    const save = vi
      .spyOn(commands, 'saveLetteringSettings')
      .mockImplementation(async (settings) => settings)
    render(<SfxPreferences />)
    const user = userEvent.setup()
    const picker = screen.getByRole('combobox', { name: 'SFX translation' })
    await waitFor(() => expect(picker).toBeEnabled())
    await user.click(picker)
    await user.click(await screen.findByRole('option', { name: /^Translate all/ }))
    await waitFor(() =>
      expect(save).toHaveBeenLastCalledWith({
        sfx: 'translate_all',
        model_confidence: 0.6,
        small_text_threshold: 0.012,
      }),
    )
    await waitFor(() => expect(picker).toHaveTextContent('Translate all'))
  })
})
