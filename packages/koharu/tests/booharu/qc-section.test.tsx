import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { ReactNode } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { BoxDetailsDialog } from '@/components/booharu/details/BoxDetailsDialog'
import { openBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { useBooharuQc } from '@/lib/booharu/qc'
import { queryClient } from '@/lib/queries'
import { commands, type Finding } from '@koharu/ipc/protocol'

import { boxesPage, change, desktop, openProject } from './boxes'
import { finding, forgetFindings } from './qc'

const overflow = finding('page', 'second')
const flag = finding('page', 'second', { kind: 'flag', message: 'Check the name' })
const small = finding(
  'page',
  'second',
  { kind: 'too_small', size: 9.5, limit: 12 },
  { hidden: true },
)

/** The findings the desktop told for the boxes page, as the store holds them. */
function hold(findings: Finding[]) {
  act(() => useBooharuQc.setState({ project: 'Demo', pages: { page: findings } }))
}

function mount(ui: ReactNode) {
  render(<QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>)
}

/** Box details, once it shows. */
function details(): Promise<HTMLElement> {
  return screen.findByRole('dialog', { name: 'Box details' })
}

/** Box details' QC section. */
async function qcSection() {
  return within(within(await details()).getByRole('region', { name: 'QC' }))
}

beforeEach(() => {
  forgetFindings()
  useBoxDetails.setState({ shown: null, reveal: null })
})

describe('Booharu Box details QC section', () => {
  it('lists the text’s open findings, its flags and those marked OK, each with its action', async () => {
    desktop()
    const mark = vi.spyOn(commands, 'booharuMarkQcOk').mockResolvedValue(change)
    const dismiss = vi.spyOn(commands, 'booharuDismissFlag').mockResolvedValue(change)
    openProject(boxesPage())
    hold([overflow, flag, small])
    const user = userEvent.setup()
    mount(<BoxDetailsDialog />)

    act(() => openBoxDetails('page', 'second'))
    const dialog = await details()
    // It follows the text's source and translation.
    expect(
      Array.from(dialog.querySelectorAll('[data-section]'), (section) =>
        section.getAttribute('data-section'),
      ),
    ).toEqual(['source', 'qc', 'explain', 'lettering', 'note', 'actions'])
    const qc = await qcSection()
    // Box details shows the text already, so no row shows it.
    expect(qc.queryByRole('button', { name: /second text/ })).toBeNull()

    await user.click(qc.getByRole('button', { name: 'Mark Text overflows the box OK' }))
    expect(mark).toHaveBeenCalledWith('second', 'overflow', true)
    await user.click(qc.getByRole('button', { name: 'Dismiss review flag: Check the name' }))
    expect(dismiss).toHaveBeenCalledWith('second', 'Check the name')
    const hidden = within(qc.getByRole('region', { name: 'Hidden (marked OK)' }))
    await user.click(hidden.getByRole('button', { name: 'Undo OK for Small text: 9.5 px' }))
    expect(mark).toHaveBeenLastCalledWith('second', 'too_small', false)

    // The desktop's check after Mark OK moves the finding under Hidden.
    hold([{ ...overflow, hidden: true }, flag, small])
    expect(
      within(qc.getByRole('region', { name: 'Hidden (marked OK)' }))
        .getAllByRole('button')
        .map((button) => button.getAttribute('aria-label')),
    ).toEqual(['Undo OK for Text overflows the box', 'Undo OK for Small text: 9.5 px'])
    expect(qc.getByRole('button', { name: 'Dismiss review flag: Check the name' })).toBeVisible()

    // Nothing found, as on a text the check leaves alone, it says so.
    act(() => openBoxDetails('page', 'first'))
    await waitFor(async () =>
      expect((await qcSection()).getByText('No issues on this box.')).toBeVisible(),
    )
    act(() => openBoxDetails('page', 'caption'))
    await waitFor(async () =>
      expect((await qcSection()).getByText('No issues on this box.')).toBeVisible(),
    )
  })
})
