import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { SeriesTermSync } from '@/components/context/SeriesTermSync'
import { queryClient } from '@/lib/queries'
import translations from '@/public/locales/en-US/translation.json'
import { commands, type TermConflict, type TermConflictChoice } from '@koharu/bridge/protocol'

const conflict: TermConflict = {
  source: 'ヒナ',
  series_target: 'Hina',
  chapter_target: 'Heena',
}
const text = translations.chapterContext

function setup(conflicts: TermConflict[] = []) {
  return render(
    <QueryClientProvider client={queryClient}>
      <SeriesTermSync conflicts={conflicts} />
    </QueryClientProvider>,
  )
}

describe('series term sync', () => {
  it('saves through the native merge and prevents overlapping actions', async () => {
    let finish!: (value: { added: number; conflicts: TermConflict[] }) => void
    const save = vi.spyOn(commands, 'saveChapterTermsToSeries').mockImplementation(
      () =>
        new Promise((resolve) => {
          finish = resolve
        }),
    )
    setup([conflict])
    fireEvent.click(screen.getByRole('button', { name: text.saveTermsToSeries }))
    await waitFor(() => expect(save).toHaveBeenCalledOnce())
    for (const button of screen.getAllByRole('button')) expect(button).toBeDisabled()
    finish({ added: 2, conflicts: [conflict] })
    await waitFor(() => expect(screen.getByRole('status')).toBeInTheDocument())
    await waitFor(() =>
      expect(screen.getByRole('button', { name: text.saveTermsToSeries })).toBeEnabled(),
    )
  })

  it.each<TermConflictChoice>(['keep_series', 'use_chapter', 'leave'])(
    'resolves %s with both observed targets so stale choices can be rejected',
    async (choice) => {
      const resolve = vi.spyOn(commands, 'resolveSeriesTermConflict').mockResolvedValue(null)
      const view = setup([conflict])
      fireEvent.click(screen.getByRole('button', { name: text.termConflictChoices[choice] }))
      await waitFor(() => expect(resolve).toHaveBeenCalledWith(conflict, choice))
      view.rerender(
        <QueryClientProvider client={queryClient}>
          <SeriesTermSync conflicts={[]} />
        </QueryClientProvider>,
      )
      expect(screen.queryByRole('region', { name: text.termConflicts })).not.toBeInTheDocument()
    },
  )
})
