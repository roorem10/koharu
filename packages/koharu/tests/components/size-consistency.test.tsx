import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { describe, expect, it, vi } from 'vitest'

import { SizeConsistencyDialog } from '@/components/lettering/SizeConsistencyDialog'
import { CategorySizePreferences } from '@/components/preferences/CategorySizePreferences'
import { queryClient } from '@/lib/queries'
import translations from '@/public/locales/en-US/translation.json'
import { commands, type LetteringSettings } from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

const labels = translations.sizeConsistency
const initial: LetteringSettings = {
  sfx: 'skip',
  model_confidence: 0.6,
  small_text_threshold: 0.012,
  category_sizes: {
    normal_dialogue: { base_size: 0.016, may_grow: false },
    shout: { base_size: 0.02, may_grow: true },
  },
}

function show(children: ReactNode) {
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>{children}</TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('Category size settings', () => {
  it('stores page fractions on commit, preserves other categories, and can opt out', async () => {
    vi.mocked(commands.getLetteringSettings).mockResolvedValue(structuredClone(initial))
    const save = vi.spyOn(commands, 'saveLetteringSettings').mockImplementation(async (settings) => settings)
    show(<CategorySizePreferences />)
    const user = userEvent.setup()
    const size = screen.getByRole('spinbutton', { name: labels.baseSize })
    await waitFor(() => expect(size).toHaveValue(1.6))
    await user.clear(size)
    await user.type(size, '1.8')
    expect(save).not.toHaveBeenCalled()
    await user.tab()
    await waitFor(() => expect(save).toHaveBeenLastCalledWith({
      ...initial,
      category_sizes: { ...initial.category_sizes, normal_dialogue: { base_size: 1.8 / 100, may_grow: false } },
    }))
    await waitFor(() => expect(size).toBeEnabled())
    await user.selectOptions(screen.getByRole('combobox', { name: labels.category }), 'shout')
    await waitFor(() => expect(size).toHaveValue(2))
    await user.click(screen.getByRole('switch', { name: labels.mayGrow }))
    await waitFor(() => expect(save.mock.lastCall?.[0].category_sizes?.shout?.may_grow).toBe(false))
    await waitFor(() => expect(size).toBeEnabled())
    await user.clear(size)
    await user.tab()
    await waitFor(() => expect(save.mock.lastCall?.[0].category_sizes?.shout).toEqual({ base_size: null, may_grow: false }))
    expect(save.mock.lastCall?.[0].category_sizes?.normal_dialogue?.base_size).toBeCloseTo(0.018)
  })

  it('cancels a draft and rejects out-of-range sizes without saving', async () => {
    vi.mocked(commands.getLetteringSettings).mockResolvedValue(structuredClone(initial))
    const save = vi.spyOn(commands, 'saveLetteringSettings').mockImplementation(async (settings) => settings)
    show(<CategorySizePreferences />)
    const user = userEvent.setup()
    const size = screen.getByRole('spinbutton', { name: labels.baseSize })
    await waitFor(() => expect(size).toHaveValue(1.6))
    await user.clear(size)
    await user.type(size, '9{Escape}')
    expect(size).toHaveValue(1.6)
    await user.clear(size)
    await user.type(size, '101')
    await user.tab()
    expect(size).toHaveValue(1.6)
    expect(save).not.toHaveBeenCalled()
  })
})

describe('Consistency edit scope', () => {
  it.each(['page', 'chapter'] as const)('submits %s and closes after refreshing the edit', async (scope) => {
    const apply = vi.spyOn(commands, 'makeSizesConsistent').mockResolvedValue({
      changed: 2, skipped: 0, kept_manual: 1, median_fraction: 0.016,
    })
    const close = vi.fn()
    show(<SizeConsistencyDialog open onOpenChange={close} page='page-1' />)
    const user = userEvent.setup()
    await user.selectOptions(screen.getByRole('combobox', { name: labels.scope }), scope)
    await user.click(screen.getByRole('button', { name: labels.action }))
    await waitFor(() => expect(apply).toHaveBeenCalledWith(scope === 'page' ? ['page-1'] : null))
    await waitFor(() => expect(close).toHaveBeenCalledWith(false))
  })
})
