import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { GlossaryDialog } from '@/components/booharu/context/GlossaryDialog'
import { SettingsPage } from '@/components/preferences/SettingsPage'
import { queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import { commands, type Preferences, type Series } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { blueLock, contextDesktop, haikyu, openChapter, term } from './context'

/** The glossary editor, open on the series `initial`, once it shows it. */
async function editor(initial: string = blueLock().id) {
  render(
    <QueryClientProvider client={queryClient}>
      <GlossaryDialog initial={initial} onClose={() => undefined} />
    </QueryClientProvider>,
  )
  const dialog = await screen.findByRole('dialog', { name: 'Glossary' })
  await within(dialog).findByRole('textbox', { name: 'Series name' })
  return within(dialog)
}

/** Blue Lock as Booharu 0.0.4 exported it, under its old name, with a term more. */
function exported(): Series {
  const series = blueLock()
  return {
    ...series,
    name: 'Blue Lock (0.0.4)',
    glossary: [...series.glossary, term('t3', '凛', 'Rin', { kind: 'name' })],
  }
}

/** The preferences with `instructions` as the translator's Instructions. */
function preferences(instructions: string): Preferences {
  return {
    pipeline: { translator: { instructions } },
    renderer: { font_families: [] },
    agent: {},
    providers: {},
  }
}

describe('Booharu glossary files', () => {
  it('exports the series shown, and says so once the file is written', async () => {
    contextDesktop()
    const exportSeries = vi.spyOn(commands, 'booharuExportSeries').mockResolvedValue(true)
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    const dialog = await editor()

    await user.click(dialog.getByRole('button', { name: 'Export series' }))
    await waitFor(() => expect(exportSeries).toHaveBeenCalledExactlyOnceWith(blueLock().id))
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith({ type: 'success', title: 'Exported “Blue Lock”.' }),
    )

    // Cancelling the file dialog writes nothing, and says nothing.
    exportSeries.mockResolvedValue(false)
    await waitFor(() => expect(dialog.getByRole('button', { name: 'Export series' })).toBeEnabled())
    await user.click(dialog.getByRole('button', { name: 'Export series' }))
    await waitFor(() => expect(exportSeries).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(dialog.getByRole('button', { name: 'Export series' })).toBeEnabled())
    expect(shown).toHaveBeenCalledOnce()
  })

  it('previews a file whose series is saved, then replaces it or keeps both', async () => {
    const { files } = contextDesktop()
    vi.spyOn(commands, 'booharuPreviewSeriesImport').mockResolvedValue({
      series: exported(),
      exists: true,
    })
    const save = vi
      .spyOn(commands, 'booharuImportSeries')
      .mockImplementation(async (series, replace) => {
        const saved = replace ? series : { ...series, id: 'blue-lock-copy-00000000' }
        files.series.set(saved.id!, structuredClone(saved))
        return structuredClone(saved)
      })
    const user = userEvent.setup()
    const dialog = await editor(haikyu().id)

    await user.click(dialog.getByRole('button', { name: 'Import series' }))
    let preview = within(await screen.findByRole('dialog', { name: 'Import series' }))
    expect(preview.getByText('Blue Lock (0.0.4)')).toBeInTheDocument()
    expect(preview.getByText('3 terms · 1 character')).toBeInTheDocument()
    expect(
      preview.getByText(
        'This series is already saved as “Blue Lock”. Replace it, or keep both copies.',
      ),
    ).toBeInTheDocument()
    expect(preview.queryByRole('button', { name: 'Import' })).toBeNull()
    await user.click(preview.getByRole('button', { name: 'Replace' }))

    await waitFor(() => expect(save).toHaveBeenCalledExactlyOnceWith(exported(), true))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Import series' })).toBeNull())
    // The editor shows the series as imported.
    await waitFor(() =>
      expect(dialog.getByRole('textbox', { name: 'Series name' })).toHaveValue('Blue Lock (0.0.4)'),
    )
    expect(files.series.get(blueLock().id)!.glossary).toHaveLength(3)

    await user.click(dialog.getByRole('button', { name: 'Import series' }))
    preview = within(await screen.findByRole('dialog', { name: 'Import series' }))
    await user.click(preview.getByRole('button', { name: 'Import as separate copy' }))

    await waitFor(() => expect(save).toHaveBeenLastCalledWith(exported(), false))
    await waitFor(() =>
      expect(dialog.getByRole('combobox', { name: 'Series' })).toHaveTextContent(
        'Blue Lock (0.0.4)',
      ),
    )
    expect([...files.series.keys()]).toEqual([
      blueLock().id,
      haikyu().id,
      'blue-lock-copy-00000000',
    ])
  })

  it('imports a series not saved yet, and nothing when the file dialog is cancelled', async () => {
    const { files } = contextDesktop()
    const pick = vi.spyOn(commands, 'booharuPreviewSeriesImport').mockResolvedValue(null)
    const save = vi.spyOn(commands, 'booharuImportSeries').mockImplementation(async (series) => {
      files.series.set(series.id!, structuredClone(series))
      return structuredClone(series)
    })
    const user = userEvent.setup()
    const dialog = await editor()

    await user.click(dialog.getByRole('button', { name: 'Import series' }))
    await waitFor(() => expect(pick).toHaveBeenCalledOnce())
    await waitFor(() => expect(dialog.getByRole('button', { name: 'Import series' })).toBeEnabled())
    expect(screen.queryByRole('dialog', { name: 'Import series' })).toBeNull()

    const kagurabachi = { ...haikyu(), id: 'kagurabachi-000000000000', name: 'Kagurabachi' }
    pick.mockResolvedValue({ series: kagurabachi, exists: false })
    await user.click(dialog.getByRole('button', { name: 'Import series' }))
    const preview = within(await screen.findByRole('dialog', { name: 'Import series' }))
    expect(preview.getByText('0 terms · 0 characters')).toBeInTheDocument()
    expect(preview.queryByRole('button', { name: 'Replace' })).toBeNull()
    await user.click(preview.getByRole('button', { name: 'Import' }))

    await waitFor(() => expect(save).toHaveBeenCalledExactlyOnceWith(kagurabachi, false))
    await waitFor(() =>
      expect(dialog.getByRole('textbox', { name: 'Series name' })).toHaveValue('Kagurabachi'),
    )
  })

  it('imports the ticked lines of the instructions, and Settings shows them without the lines removed', async () => {
    const before = 'Keep honorifics.\n潔 = Isagi\nエゴ = ego\n凛 = Rin'
    const after = 'Keep honorifics.\nエゴ = ego'
    const { files } = contextDesktop()
    openChapter()
    const read = vi.spyOn(commands, 'getPreferences').mockResolvedValue(preferences(before))
    const saved = vi
      .spyOn(commands, 'savePreferences')
      .mockImplementation(async (written) => written)
    vi.spyOn(commands, 'listModels').mockResolvedValue([])
    vi.spyOn(commands, 'booharuPreviewInstructionGlossary').mockResolvedValue({
      instructions: before,
      terms: [
        { line: 1, source: '潔', target: 'Isagi' },
        { line: 2, source: 'エゴ', target: 'ego' },
        { line: 3, source: '凛', target: 'Rin' },
      ],
    })
    const imported = vi
      .spyOn(commands, 'booharuImportInstructionGlossary')
      .mockImplementation(async (id) => {
        // The desktop saves the series, and the Instructions without the lines imported.
        const series = files.series.get(id)!
        series.glossary = [...series.glossary!, term('t3', '凛', 'Rin', { locked: true })]
        read.mockResolvedValue(preferences(after))
        return { series: structuredClone(series), added: 1, skipped: 1, instructions: after }
      })
    const shown = vi.spyOn(toast, 'add')
    useKoharuStore.setState({ settingsOpen: true, preferences: preferences(before) })
    const user = userEvent.setup()
    render(
      <QueryClientProvider client={queryClient}>
        <SettingsPage />
      </QueryClientProvider>,
    )
    await user.click(screen.getByRole('button', { name: 'Pipeline' }))
    const field = await screen.findByRole('textbox', { name: 'Additional instructions' })
    expect(field).toHaveValue(before)

    await user.click(screen.getByRole('button', { name: 'Open full glossary' }))
    const editor = within(await screen.findByRole('dialog', { name: 'Glossary' }))
    await editor.findByRole('textbox', { name: 'Series name' })
    await user.click(editor.getByRole('button', { name: 'Import from instructions' }))
    const preview = within(await screen.findByRole('dialog', { name: 'Import from instructions' }))
    expect(
      preview.getByText('Imported entries are locked. Existing terms are kept.'),
    ).toBeInTheDocument()
    const lines = preview.getAllByRole('listitem').map((line) => line.textContent)
    expect(lines).toEqual(['潔 → Isagi Line 2', 'エゴ → ego Line 3', '凛 → Rin Line 4'])
    const ego = preview.getByRole('checkbox', { name: 'エゴ → ego Line 3' })
    expect(ego).toBeChecked()
    await user.click(ego)
    expect(ego).not.toBeChecked()
    await user.click(
      preview.getByRole('checkbox', { name: 'Remove these lines from the instructions' }),
    )
    const savesBefore = saved.mock.calls.length
    await user.click(preview.getByRole('button', { name: 'Import selected' }))

    await waitFor(() =>
      expect(imported).toHaveBeenCalledExactlyOnceWith(blueLock().id, [1, 3], true, before),
    )
    await waitFor(() =>
      expect(screen.queryByRole('dialog', { name: 'Import from instructions' })).toBeNull(),
    )
    expect(shown).toHaveBeenCalledWith({
      type: 'success',
      title: 'Imported 1 term.',
      description:
        'Skipped 1 term the series already had. Removed the imported lines from the instructions.',
    })
    // The preferences are read again, so Settings shows the Instructions as saved now…
    await waitFor(() => expect(field).toHaveValue(after))
    expect(useKoharuStore.getState().preferences).toEqual(preferences(after))
    // …and its draft, the old text, is never saved back over them.
    await act(() => new Promise((resolve) => setTimeout(resolve, 400)))
    expect(
      saved.mock.calls
        .slice(savesBefore)
        .filter(([written]) => written.pipeline.translator?.instructions === before),
    ).toEqual([])
  })

  it('keeps the instructions as they are when the lines stay', async () => {
    contextDesktop()
    const read = vi.spyOn(commands, 'getPreferences')
    vi.spyOn(commands, 'booharuPreviewInstructionGlossary').mockResolvedValue({
      instructions: '潔 = Isagi',
      terms: [{ line: 0, source: '潔', target: 'Isagi' }],
    })
    const imported = vi
      .spyOn(commands, 'booharuImportInstructionGlossary')
      .mockResolvedValue({ series: blueLock(), added: 0, skipped: 1, instructions: null })
    const shown = vi.spyOn(toast, 'add')
    const user = userEvent.setup()
    const dialog = await editor()

    await user.click(dialog.getByRole('button', { name: 'Import from instructions' }))
    const preview = within(await screen.findByRole('dialog', { name: 'Import from instructions' }))
    await user.click(preview.getByRole('button', { name: 'Import selected' }))

    await waitFor(() =>
      expect(imported).toHaveBeenCalledExactlyOnceWith(blueLock().id, [0], false, '潔 = Isagi'),
    )
    await waitFor(() =>
      expect(shown).toHaveBeenCalledWith({
        type: 'success',
        title: 'Imported 0 terms.',
        description: 'Skipped 1 term the series already had.',
      }),
    )
    expect(read).not.toHaveBeenCalled()
  })

  it('says when the instructions hold no glossary lines', async () => {
    contextDesktop()
    vi.spyOn(commands, 'booharuPreviewInstructionGlossary').mockResolvedValue({
      instructions: 'Keep honorifics.',
      terms: [],
    })
    const user = userEvent.setup()
    const dialog = await editor()

    await user.click(dialog.getByRole('button', { name: 'Import from instructions' }))
    const preview = within(await screen.findByRole('dialog', { name: 'Import from instructions' }))
    expect(
      preview.getByText('No glossary entries found in the translation instructions.'),
    ).toBeInTheDocument()
    expect(preview.queryByRole('button', { name: 'Import selected' })).toBeNull()
    expect(preview.queryByRole('checkbox')).toBeNull()
  })
})
