import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypesettingPreferences } from '@/components/preferences/TypesettingPreferences'
import { queryClient } from '@/lib/queries'
import type { LetteringSettings } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { builtInLettering, builtInRuns, settingsDesktop, settle } from './settings'

const RED = [1, 0, 0, 1] as const
const WHITE = [1, 1, 1, 1] as const

/** A default text style with red text and a white outline 3 px wide. */
const styled = {
  ...builtInLettering,
  default: { fill: [...RED], stroke: [...WHITE], stroke_width: 3 },
} satisfies LetteringSettings

/** Settings > Typesetting, where the default text style sits below Preferred fonts. */
function renderTypesetting() {
  return render(
    <QueryClientProvider client={queryClient}>
      <TypesettingPreferences
        value={{ font_families: ['CCWildWords'] }}
        onChange={() => undefined}
      />
    </QueryClientProvider>,
  )
}

/** The default text style section, once it shows the file's style. */
async function section() {
  const region = await screen.findByRole('region', { name: 'Default text style' })
  await waitFor(() =>
    expect(within(region).getByRole('button', { name: 'Text color' })).toBeEnabled(),
  )
  return within(region)
}

/** Pastes `hex` into the colour well named `well`, as picking a colour in it does. */
async function pick(user: ReturnType<typeof userEvent.setup>, well: string, hex: string) {
  await user.click(screen.getByRole('button', { name: well }))
  const input = await screen.findByRole('textbox', { name: 'Hex color code' })
  await user.clear(input)
  // Whole, as typing it would pick each three-digit colour on the way.
  await user.paste(hex)
  await user.keyboard('{Escape}')
  await waitFor(() => expect(screen.queryByRole('textbox', { name: 'Hex color code' })).toBeNull())
}

describe('Booharu default text style', () => {
  it("shows black and None while unset, and saves a changed colour over every section's latest value", async () => {
    const { file, saves } = settingsDesktop()
    const user = userEvent.setup()
    renderTypesetting()

    const shown = await section()
    expect(shown.getByText('#000000')).toBeInTheDocument()
    expect(shown.getByText('None')).toBeInTheDocument()
    expect(shown.getByRole('button', { name: 'Clear all' })).toBeDisabled()

    // Picking the black an unset colour shows leaves it unset.
    await pick(user, 'Text color', '000000')
    await settle()
    expect(saves).toEqual([])

    // Another writer changes another section, and another key of this one, after the page read.
    file.preferences['booharu-mcp'] = { port: 50000 }
    file.preferences['booharu-lettering'].detect_colors = false
    await pick(user, 'Text color', 'FF0000')

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': {
        ...builtInLettering,
        default: { fill: [...RED], stroke: null, stroke_width: 1.5 },
        detect_colors: false,
      },
      'booharu-fonts': { favorites: [] },
      'booharu-runs': builtInRuns,
    })
    expect(shown.getByText('#FF0000')).toBeInTheDocument()
    expect(shown.getByText('None')).toBeInTheDocument()
  })

  it('clears one group, then every group at once', async () => {
    const { saves } = settingsDesktop(styled)
    const user = userEvent.setup()
    renderTypesetting()

    const shown = await section()
    expect(shown.getByText('#FF0000')).toBeInTheDocument()
    expect(shown.getByText('#FFFFFF')).toBeInTheDocument()
    expect(shown.getByRole('textbox', { name: 'Outline width' })).toHaveValue('3')

    await user.click(shown.getByRole('button', { name: 'Clear text color' }))
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!['booharu-lettering'].default).toEqual({
      fill: null,
      stroke: [...WHITE],
      stroke_width: 3,
    })
    expect(shown.getByText('#000000')).toBeInTheDocument()
    expect(shown.getByRole('button', { name: 'Clear text color' })).toBeDisabled()

    await user.click(shown.getByRole('button', { name: 'Clear all' }))
    await waitFor(() => expect(saves).toHaveLength(2))
    expect(saves[1]!['booharu-lettering']).toEqual(builtInLettering)
    expect(shown.getByText('None')).toBeInTheDocument()
    expect(shown.getByRole('textbox', { name: 'Outline width' })).toHaveValue('1.5')
    expect(shown.getByRole('button', { name: 'Clear outline' })).toBeDisabled()
    expect(shown.getByRole('button', { name: 'Clear all' })).toBeDisabled()
  })

  it('clears the outline back to none and its built-in width', async () => {
    const { saves } = settingsDesktop(styled)
    const user = userEvent.setup()
    renderTypesetting()

    const shown = await section()
    await user.click(shown.getByRole('button', { name: 'Clear outline' }))

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!['booharu-lettering'].default).toEqual({
      fill: [...RED],
      stroke: null,
      stroke_width: 1.5,
    })
    expect(shown.getByText('None')).toBeInTheDocument()
    expect(shown.getByText('#FF0000')).toBeInTheDocument()
  })

  it('keeps an outline width within 0 to 50, and saves nothing while the field is blank', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    renderTypesetting()

    const shown = await section()
    const width = shown.getByRole('textbox', { name: 'Outline width' })
    expect(width).toHaveValue('1.5')

    await user.clear(width)
    await settle()
    expect(saves).toEqual([])

    // A width set first gives the outline a colour: white, as the TypeBar's border gets.
    await user.type(width, '60')
    await user.tab()
    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]!['booharu-lettering'].default).toEqual({
      fill: null,
      stroke: [...WHITE],
      stroke_width: 50,
    })
    expect(width).toHaveValue('50')
    expect(shown.getByText('#FFFFFF')).toBeInTheDocument()
  })

  it('saves a change still waiting at once when the section goes, such as when another tab opens', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    const { unmount } = renderTypesetting()

    const shown = await section()
    await user.click(shown.getByRole('button', { name: 'Increase outline width' }))
    unmount()
    // Long before the change's own save would come.
    await new Promise((resolve) => setTimeout(resolve, 0))

    expect(saves).toHaveLength(1)
    expect(saves[0]!['booharu-lettering'].default).toEqual({
      fill: null,
      stroke: [...WHITE],
      stroke_width: 2,
    })
    await settle()
    expect(saves).toHaveLength(1)
  })

  it("shows a failed save's error, then the file's style", async () => {
    const { save } = settingsDesktop()
    save.mockRejectedValue('config.toml is read-only')
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    renderTypesetting()

    const region = await section()
    await pick(user, 'Text color', 'FF0000')
    expect(region.getByText('#FF0000')).toBeInTheDocument()

    // As every failed command shows its error.
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'error', description: 'config.toml is read-only' }),
      ),
    )
    await waitFor(() => expect(region.getByText('#000000')).toBeInTheDocument())
  })
})
