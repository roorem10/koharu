import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { letteringCategoriesSection } from '@/lib/booharu/categories'
import { openSettingsSection, useSettingsLinks, type SettingsTab } from '@/lib/booharu/settingsLink'
import { useKoharuStore } from '@/lib/store'

/** A stand-in for the Settings page: its tab setter handed over as SettingsPage hands it, and the
 * section a link goes to. */
function Settings({ show }: { show: (tab: SettingsTab) => void }) {
  useSettingsLinks(show)
  return <section id={letteringCategoriesSection} tabIndex={-1} aria-label='Lettering categories' />
}

describe('Booharu links to a Settings section', () => {
  it('opens Settings from the editor, then shows the tab and the section', async () => {
    const show = vi.fn()
    openSettingsSection('typesetting', letteringCategoriesSection)
    expect(useKoharuStore.getState().settingsOpen).toBe(true)

    render(<Settings show={show} />)
    expect(show).toHaveBeenCalledExactlyOnceWith('typesetting')
    const section = screen.getByRole('region', { name: 'Lettering categories' })
    await waitFor(() => expect(section).toHaveFocus())
    expect(section.scrollIntoView).toHaveBeenCalledWith({ block: 'start' })
  })

  it('shows the tab at once while Settings is open', async () => {
    const show = vi.fn()
    // No link waits: Settings opened by hand shows the tab it's on.
    render(<Settings show={show} />)
    expect(show).not.toHaveBeenCalled()

    openSettingsSection('typesetting', letteringCategoriesSection)
    expect(show).toHaveBeenCalledExactlyOnceWith('typesetting')
    await waitFor(() =>
      expect(screen.getByRole('region', { name: 'Lettering categories' })).toHaveFocus(),
    )
    expect(useKoharuStore.getState().settingsOpen).toBe(false)
  })
})
