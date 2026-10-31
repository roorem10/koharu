import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { EmphasisPreferences } from '@/components/emphasis/EmphasisPreferences'
import { emptyEmphasisStyle } from '@/components/emphasis/settings'
import { fontsKey, queryClient } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

import { emphasisDefaults } from '../mocks/wo11f'

function show() {
  queryClient.setQueryData(fontsKey, [])
  return render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider>
        <EmphasisPreferences />
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('Emphasis settings', () => {
  it('loads defaults, saves the AI toggle, and saves a category without altering the default', async () => {
    show()
    const user = userEvent.setup()
    await user.click(await screen.findByRole('checkbox', { name: 'Let the AI emphasise words' }))
    await waitFor(() =>
      expect(commands.saveEmphasisSettings).toHaveBeenLastCalledWith({
        ...emphasisDefaults(),
        enabled_for_ai: false,
      }),
    )
    const category = screen.getByRole('combobox', { name: 'Lettering category' })
    expect(category.querySelectorAll('option')).toHaveLength(16)
    await user.selectOptions(category, 'shout')
    const size = screen.getByRole('spinbutton', { name: 'Size (%)' })
    expect(size).toHaveAttribute('placeholder', 'Same as default')
    await user.type(size, '130.5')
    await user.tab()
    await waitFor(() =>
      expect(commands.saveEmphasisSettings).toHaveBeenLastCalledWith({
        ...emphasisDefaults(),
        enabled_for_ai: false,
        categories: { shout: { ...emptyEmphasisStyle, size_scale: 1.305 } },
      }),
    )
    await user.click(screen.getByRole('button', { name: 'Use default emphasis for this category' }))
    await waitFor(() =>
      expect(commands.saveEmphasisSettings).toHaveBeenLastCalledWith({
        ...emphasisDefaults(),
        enabled_for_ai: false,
      }),
    )
  })

  it('clears an inherited field, cancels with Escape, and rejects an invalid value', async () => {
    const saved = emphasisDefaults()
    saved.categories = { shout: { ...saved.default, size_scale: 1.3 } }
    vi.mocked(commands.getEmphasisSettings).mockResolvedValue(saved)
    show()
    const user = userEvent.setup()
    await user.selectOptions(
      await screen.findByRole('combobox', { name: 'Lettering category' }),
      'shout',
    )
    const size = screen.getByRole('spinbutton', { name: 'Size (%)' })
    expect(size).toHaveValue(130)
    await user.clear(size)
    await user.type(size, '200')
    await user.keyboard('{Escape}')
    expect(size).toHaveValue(130)
    expect(commands.saveEmphasisSettings).not.toHaveBeenCalled()
    fireEvent.change(size, { target: { value: '-20' } })
    fireEvent.blur(size)
    expect(size).toHaveValue(130)
    expect(commands.saveEmphasisSettings).not.toHaveBeenCalled()
    await user.clear(size)
    await user.tab()
    await waitFor(() =>
      expect(commands.saveEmphasisSettings).toHaveBeenLastCalledWith({
        ...saved,
        categories: { shout: { ...saved.default, size_scale: null } },
      }),
    )
  })

  it('serializes quick saves and keeps the latest edit while an older save completes', async () => {
    let finish!: () => void
    const pending = new Promise<void>((resolve) => {
      finish = resolve
    })
    const save = vi.mocked(commands.saveEmphasisSettings)
    save.mockImplementationOnce(async (settings) => {
      await pending
      return settings
    })
    show()
    const user = userEvent.setup()
    const toggle = await screen.findByRole('checkbox', { name: 'Let the AI emphasise words' })
    await user.click(toggle)
    await waitFor(() => expect(save).toHaveBeenCalledTimes(1))
    await user.click(toggle)
    expect(toggle).toBeChecked()
    expect(save).toHaveBeenCalledTimes(1)
    finish()
    await waitFor(() => expect(save).toHaveBeenCalledTimes(2))
    expect(save.mock.lastCall?.[0].enabled_for_ai).toBe(true)
    expect(toggle).toBeChecked()
  })

  it('reloads the saved value after a failed write', async () => {
    vi.mocked(commands.saveEmphasisSettings).mockRejectedValueOnce(new Error('save failed'))
    show()
    const user = userEvent.setup()
    const toggle = await screen.findByRole('checkbox', { name: 'Let the AI emphasise words' })
    await user.click(toggle)
    await waitFor(() => expect(commands.getEmphasisSettings).toHaveBeenCalledTimes(2))
    expect(toggle).toBeChecked()
  })
})
