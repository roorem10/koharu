import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { TitleBar } from '@/components/app/TitleBar'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'

import { boxesPage, desktop, openProject } from './boxes'
import { settle } from './reader'

/** Booharu's items in the Edit menu, in order. */
const boxItems = [
  'FX…',
  'Read again',
  'Translate again',
  'Clean again',
  'Join with balloon',
  'Split',
  'Add subtitle',
  'Change category…',
  'Reset to lettering default',
  'Explain this line',
  'Box details',
  'Join split texts on this page',
]

/** Booharu's items in the Edit menu open now, by name, each with whether it's enabled. */
async function booharuItems(): Promise<[string, boolean][]> {
  const open = () =>
    screen
      .queryAllByRole('menuitem')
      .filter((item) => item.closest('[data-slot$="-content"]'))
      .filter((item) => boxItems.includes(item.textContent ?? ''))
  await waitFor(() => expect(open()).toHaveLength(boxItems.length))
  return open().map((item) => [
    item.textContent ?? '',
    item.getAttribute('aria-disabled') !== 'true',
  ])
}

describe("Booharu's Edit menu items in Koharu's Read mode", () => {
  it('greys them out while reading and gives them back in Edit', async () => {
    const { findSplit } = desktop()
    openProject(boxesPage())
    // A block's clean-up stands for the block: in Edit, most of the box actions take it.
    useKoharuStore.getState().selectLayers(['first-cleanup'])
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <TitleBar />
      </QueryClientProvider>,
    )

    await user.click(screen.getByRole('button', { name: 'Read' }))
    expect(useKoharuStore.getState().reading).toBe(true)
    await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
    // Given time to read the page's split balloons, it reads none: nothing it would act on shows.
    await settle()
    expect(await booharuItems()).toEqual(boxItems.map((name) => [name, false]))
    expect(findSplit).not.toHaveBeenCalled()
    await user.keyboard('{Escape}')

    await user.click(screen.getByRole('button', { name: 'Edit' }))
    expect(useKoharuStore.getState().reading).toBe(false)
    await user.click(screen.getByRole('menuitem', { name: 'Edit' }))
    await waitFor(async () =>
      expect(Object.fromEntries(await booharuItems())).toMatchObject({
        'Clean again': true,
        'Join split texts on this page': true,
      }),
    )
    expect(findSplit).toHaveBeenCalledWith('page')
  })
})
