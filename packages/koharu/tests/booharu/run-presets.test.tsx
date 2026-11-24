import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import * as reactI18next from 'react-i18next'
import { describe, expect, it, vi } from 'vitest'

import { InferenceControl } from '@/components/editor/InferenceControl'
import { resources } from '@/lib/i18n'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands } from '@koharu/ipc/protocol'

import {
  builtInEmphasis,
  builtInLettering,
  builtInLevels,
  builtInReading,
  builtInRuns,
  settingsDesktop,
} from './settings'

/** The interface's strings, as the tests' setup has them. */
const { i18n } = reactI18next.useTranslation()

/** The Run control, with its menu open on its root view once the presets show. */
async function openRunMenu(user: ReturnType<typeof userEvent.setup>) {
  vi.spyOn(commands, 'listModels').mockResolvedValue([])
  const onRun = vi.fn()
  render(
    <QueryClientProvider client={queryClient}>
      <InferenceControl onRun={onRun} disabled={false} />
    </QueryClientProvider>,
  )
  await user.click(screen.getByRole('button', { name: i18n.t('inference.selector') }))
  const group = await screen.findByRole('group', { name: i18n.t('booharu.runs.presets.short') })
  return { onRun, presets: within(group) }
}

const stages = () => useKoharuStore.getState().processingStages

describe('Booharu run presets in the Run menu', () => {
  it('switches the stages to a preset’s and starts nothing', async () => {
    settingsDesktop()
    const user = userEvent.setup()
    const { onRun, presets } = await openRunMenu(user)

    const detect = presets.getByRole('button', { name: 'Detect + OCR' })
    const translate = presets.getByRole('button', { name: 'Translate + clean up' })
    expect(detect).toHaveAttribute('aria-pressed', 'false')

    await user.click(translate)
    expect(stages()).toEqual(['inpaint', 'translate'])
    expect(translate).toHaveAttribute('aria-pressed', 'true')
    expect(screen.getByRole('button', { name: 'Stages 2 stages' })).toBeInTheDocument()

    await user.click(detect)
    expect(stages()).toEqual(['detect', 'recognize'])
    expect(detect).toHaveAttribute('aria-pressed', 'true')
    expect(translate).toHaveAttribute('aria-pressed', 'false')

    expect(onRun).not.toHaveBeenCalled()
    expect(commands.runPipeline).not.toHaveBeenCalled()
    expect(commands.booharuRun).not.toHaveBeenCalled()
  })

  it('saves the stages switched on as a preset, named in place', async () => {
    const { file, saves } = settingsDesktop()
    useKoharuStore.setState({ processingStages: ['recognize', 'translate'] })
    const user = userEvent.setup()
    await openRunMenu(user)
    // Another writer changes another section after the menu read the file.
    file.preferences['booharu-mcp'] = { port: 50000 }

    await user.click(screen.getByRole('button', { name: 'Stages 2 stages' }))
    await user.click(screen.getByRole('button', { name: 'Save as preset' }))
    await user.type(screen.getByRole('textbox', { name: 'Preset name' }), 'Read + translate{Enter}')

    await waitFor(() => expect(saves).toHaveLength(1))
    expect(saves[0]).toEqual({
      'booharu-mcp': { port: 50000 },
      'booharu-ocr-box': { default_mode: 'read' },
      'booharu-lettering': builtInLettering,
      'booharu-fonts': { favorites: [] },
      'booharu-runs': {
        ...builtInRuns,
        presets: [
          ...builtInRuns.presets,
          { name: 'Read + translate', stages: ['recognize', 'translate'] },
        ],
      },
      'booharu-reading': builtInReading,
      'booharu-qc': { min_size: 0.01 },
      'booharu-start': { fresh_start_note_seen: false },
      'booharu-emphasis': builtInEmphasis,
      'booharu-levels': builtInLevels,
    })
    // The menu goes back to its root view, where the new preset shows as the stages on.
    const saved = await within(screen.getByRole('group', { name: 'Presets' })).findByRole(
      'button',
      { name: 'Read + translate' },
    )
    expect(saved).toHaveAttribute('aria-pressed', 'true')
    expect(commands.runPipeline).not.toHaveBeenCalled()
  })

  it('reads the presets again as the menu opens, as the agent may have changed them', async () => {
    const { file } = settingsDesktop()
    const user = userEvent.setup()
    await openRunMenu(user)
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('group', { name: 'Presets' })).toBeNull())
    // The agent's settings tool adds a preset while the menu is closed.
    file.preferences['booharu-runs'].presets = [
      ...builtInRuns.presets,
      { name: 'Clean only', stages: ['inpaint'] },
    ]

    await user.click(screen.getByRole('button', { name: i18n.t('inference.selector') }))
    const presets = within(await screen.findByRole('group', { name: 'Presets' }))
    expect(await presets.findByRole('button', { name: 'Clean only' })).toBeInTheDocument()
  })

  it('shows a built-in preset’s name in the language shown only while it is the built-in one', async () => {
    const { file } = settingsDesktop()
    // The second built-in preset has had its stages changed: it's the user's own now.
    file.preferences['booharu-runs'].presets = [
      builtInRuns.presets[0],
      { name: 'Translate + clean up', stages: ['translate'] },
      { name: 'Clean only', stages: ['inpaint'] },
    ]
    i18n.addResourceBundle('ja-JP', 'translation', resources['ja-JP'].translation)
    await i18n.changeLanguage('ja-JP')
    try {
      const user = userEvent.setup()
      const { presets } = await openRunMenu(user)
      expect(presets.getAllByRole('button').map((button) => button.textContent)).toEqual([
        '検出 + OCR',
        'Translate + clean up',
        'Clean only',
      ])
    } finally {
      await i18n.changeLanguage('en-US')
    }
  })

  it('refuses a blank name, or one another preset has, and puts the button back on Escape', async () => {
    const { saves } = settingsDesktop()
    const user = userEvent.setup()
    await openRunMenu(user)

    await user.click(screen.getByRole('button', { name: 'Stages 4 stages' }))
    await user.click(screen.getByRole('button', { name: 'Save as preset' }))
    const name = screen.getByRole('textbox', { name: 'Preset name' })
    expect(name).toHaveFocus()

    await user.click(screen.getByRole('button', { name: 'Save' }))
    expect(name).toHaveAccessibleDescription('Give the preset a name.')
    await user.type(name, 'Translate + clean up{Enter}')
    expect(name).toHaveAccessibleDescription(
      'Another preset is already named “Translate + clean up”.',
    )
    expect(saves).toEqual([])

    await user.keyboard('{Escape}')
    // The menu stays open on its stages, with the button back.
    expect(screen.queryByRole('textbox', { name: 'Preset name' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Save as preset' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /^Detection/ })).toBeInTheDocument()
    // Escape from anywhere else in the menu still closes it.
    await user.keyboard('{Escape}')
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Save as preset' })).toBeNull())
  })
})
