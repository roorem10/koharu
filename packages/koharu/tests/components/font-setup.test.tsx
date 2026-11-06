import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { rankCategoryFonts } from '@/components/ready-check/font-ranking'
import { FontSetup } from '@/components/ready-check/FontSetup'
import { fontsKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { emptyCategoryStyles, emptyShapeStyles, emptyTextStyle } from '@/lib/text-styles'
import { commands, type FontFamily, type TextStyles } from '@koharu/bridge/protocol'

function font(name: string, source: 'system' | 'bundled' = 'system'): FontFamily {
  return {
    name,
    local_names: [],
    sources: [source],
    faces: [],
    metadata: {
      primary_script: null,
      scripts: [],
      languages: [],
      category: null,
      classifications: [],
      use_cases: [],
    },
  }
}

describe('font name ranking', () => {
  it('uses category-specific names first, then relevant hints, and only installed fonts', () => {
    const families = [
      font('Ordinary Serif'),
      font('Comic Handwriting'),
      font('Manga Display Bold'),
      font('Palooka BB'),
      font('Badaboom BB', 'bundled'),
    ]
    const original = structuredClone(families)
    expect(rankCategoryFonts('shout', families).map(({ family }) => family.name)).toEqual([
      'Palooka BB',
      'Manga Display Bold',
      'Comic Handwriting',
    ])
    expect(rankCategoryFonts('whisper', families)[0].family.name).toBe('Comic Handwriting')
    expect(rankCategoryFonts('phone_text', families)).toEqual([])
    expect(families).toEqual(original)
  })

  it('matches normalized local aliases and sorts equal scores independently of input order', () => {
    const local = font('Local Family')
    local.local_names = ['ＣＣ Ｗｉｌｄ Ｗｏｒｄｓ']
    const families = [font('Zeta Comic'), font('Alpha Comic'), local]
    expect(rankCategoryFonts('normal_dialogue', families).map(({ family }) => family.name)).toEqual(
      ['Local Family', 'Alpha Comic', 'Zeta Comic'],
    )
    expect(rankCategoryFonts('normal_dialogue', [...families].reverse())).toEqual(
      rankCategoryFonts('normal_dialogue', families),
    )
  })

  it('offers handwriting, heavy, screen, and display names for their categories', () => {
    const families = [
      font('Brush Script'),
      font('Impact Heavy'),
      font('Screen Mono'),
      font('Poster Display'),
    ]
    expect(rankCategoryFonts('crying', families)[0].family.name).toBe('Brush Script')
    expect(rankCategoryFonts('weak_shout', families)[0].family.name).toBe('Impact Heavy')
    expect(rankCategoryFonts('phone_text', families)[0].family.name).toBe('Screen Mono')
    expect(rankCategoryFonts('title', families)[0].family.name).toBe('Poster Display')
  })
})

describe('font setup', () => {
  it('only saves after acceptance and preserves the latest unrelated settings and chosen style properties', async () => {
    const original: TextStyles = {
      default_style: { ...emptyTextStyle },
      shape_styles: structuredClone(emptyShapeStyles),
      category_styles: structuredClone(emptyCategoryStyles),
      legacy_color_migrated: true,
    }
    original.category_styles!.whisper = {
      ...emptyTextStyle,
      fill: [1, 2, 3, 255],
      font: { preferred_font: 'Hand choice', font_weight: 600, font_style: 'italic' },
    }
    useKoharuStore.setState({ textStyles: original })
    queryClient.setQueryData(fontsKey, [font('Comic Handwriting')])
    const save = vi.spyOn(commands, 'saveTextStyles').mockImplementation(async (styles) => styles)
    const onSaved = vi.fn()
    render(
      <QueryClientProvider client={queryClient}>
        <FontSetup onSaved={onSaved} />
      </QueryClientProvider>,
    )
    const row = await screen.findByTestId('font-setup-whisper')
    expect(save).not.toHaveBeenCalled()
    const latest = structuredClone(original)
    latest.shape_styles!.round!.fill = [5, 6, 7, 255]
    useKoharuStore.setState({ textStyles: latest })
    await userEvent.setup().click(within(row).getByRole('button'))
    await waitFor(() => expect(save).toHaveBeenCalledOnce())
    expect(save.mock.lastCall![0]).toEqual({
      ...latest,
      category_styles: {
        ...latest.category_styles,
        whisper: {
          ...latest.category_styles!.whisper,
          font: { preferred_font: 'Comic Handwriting', font_weight: 600, font_style: 'italic' },
        },
      },
    })
    await waitFor(() => expect(onSaved).toHaveBeenCalledOnce())
  })
})
