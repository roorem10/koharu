import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

import { ContextPreferences } from '@/components/context/ContextPreferences'
import { LineContextCard } from '@/components/editor/LineContextCard'
import { OcrCorrectionCard } from '@/components/editor/OcrCorrectionCard'
import { FindingRow } from '@/components/qc/FindingRow'
import { projectKey, queryClient } from '@/lib/queries'
import strings from '@/public/locales/en-US/translation.json'
import { commands, type OcrSuggestion } from '@koharu/bridge/protocol'

const suggestion: OcrSuggestion = {
  layer: 'line',
  page: 'page',
  original: 'The OCR missed a character.',
  suggested: 'The OCR missed two characters.',
  reason: 'The image contains the extra character.',
  status: 'pending',
}
const project = {
  name: 'chapter',
  revision: 1,
  active_page: null,
  can_undo: false,
  can_redo: false,
}

function mount(element = <OcrCorrectionCard layer='line' />) {
  queryClient.setQueryData(projectKey, project)
  vi.spyOn(commands, 'getProject').mockResolvedValue(project)
  vi.spyOn(commands, 'getPage').mockResolvedValue(null)
  return render(<QueryClientProvider client={queryClient}>{element}</QueryClientProvider>)
}

describe('OCR correction review', () => {
  it('accepts the selected suggestion and keeps the original visible after the native commit', async () => {
    vi.mocked(commands.getOcrSuggestions).mockResolvedValue({
      suggestions: [suggestion],
      skip_reason: null,
    })
    const accept = vi.spyOn(commands, 'acceptOcrCorrection').mockImplementation(async () => {
      vi.mocked(commands.getOcrSuggestions).mockResolvedValue({
        suggestions: [{ ...suggestion, status: 'applied' }],
        skip_reason: null,
      })
      return { applied: 1, pages: 1, skip_reason: null }
    })
    mount()
    expect(await screen.findByText(suggestion.original)).toBeInTheDocument()
    expect(screen.getByText(suggestion.suggested)).toBeInTheDocument()
    expect(screen.getByText(suggestion.reason)).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: strings.ocrCorrections.accept }))
    await waitFor(() => expect(accept).toHaveBeenCalledWith('line'))
    expect(await screen.findByText(strings.ocrCorrections.corrected)).toBeInTheDocument()
    expect(screen.getByText(suggestion.original)).toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })

  it('dismisses without invoking a source edit and refreshes the sidecar state', async () => {
    vi.mocked(commands.getOcrSuggestions).mockResolvedValue({
      suggestions: [suggestion],
      skip_reason: null,
    })
    const accept = vi.spyOn(commands, 'acceptOcrCorrection')
    const dismiss = vi.spyOn(commands, 'dismissOcrCorrection').mockImplementation(async () => {
      vi.mocked(commands.getOcrSuggestions).mockResolvedValue({
        suggestions: [{ ...suggestion, status: 'dismissed' }],
        skip_reason: null,
      })
      return null
    })
    mount()
    fireEvent.click(await screen.findByRole('button', { name: strings.chapterContext.dismiss }))
    await waitFor(() => expect(dismiss).toHaveBeenCalledWith('line'))
    await waitFor(() => expect(screen.queryByText(suggestion.original)).not.toBeInTheDocument())
    expect(accept).not.toHaveBeenCalled()
  })

  it.each(['hand_edited', 'stale'] as const)(
    'does not offer an inapplicable %s correction',
    async (status) => {
      vi.mocked(commands.getOcrSuggestions).mockResolvedValue({
        suggestions: [{ ...suggestion, status }],
        skip_reason: null,
      })
      mount()
      await waitFor(() =>
        expect(commands.getOcrSuggestions).toHaveBeenCalledWith({
          scope: 'entities',
          value: ['line'],
        }),
      )
      expect(screen.queryByRole('button')).not.toBeInTheDocument()
    },
  )

  it('shows the recorded no-vision skip reason', async () => {
    vi.mocked(commands.getOcrSuggestions).mockResolvedValue({
      suggestions: [],
      skip_reason: 'no_vision_model',
    })
    mount()
    expect(await screen.findByText(strings.ocrCorrections.noVisionModel)).toBeInTheDocument()
    expect(screen.queryByRole('button')).not.toBeInTheDocument()
  })

  it('uses the same localized stale-translation flag in box details and QC', async () => {
    const flag = { kind: 'ocr_source_changed', page: 1 } as const
    vi.mocked(commands.getChapterContext).mockResolvedValue({
      context: {
        version: 1,
        series: null,
        brief: null,
        suggestions: [],
        lines: { line: { flags: [flag] } },
      },
      series: null,
      supports_tasks: false,
    })
    mount(
      <>
        <LineContextCard layer='line' />
        <FindingRow
          finding={{
            page: 'page',
            page_label: '1',
            layer: 'line',
            check: 'flag',
            detail: '',
            translation: 'Old translation',
            flag,
          }}
        />
      </>,
    )
    await waitFor(() =>
      expect(screen.getAllByText(strings.ocrCorrections.translationOutOfDate)).toHaveLength(2),
    )
  })

  it('persists automatic application without changing other translation context preferences', async () => {
    const settings = {
      read_when: 'missing',
      send_images: true,
      ask_before_reading: true,
      read_model: null,
      pages_per_request: 20,
      previous_pages: 1,
      agent_messages: 20,
      auto_apply_ocr_corrections: false,
    } as const
    vi.mocked(commands.getContextSettings).mockResolvedValue(settings)
    const save = vi
      .spyOn(commands, 'saveContextSettings')
      .mockImplementation(async (value) => value)
    mount(<ContextPreferences />)
    const checkbox = await screen.findByRole('checkbox', { name: strings.ocrCorrections.automatic })
    expect(checkbox).not.toBeChecked()
    fireEvent.click(checkbox)
    await waitFor(() =>
      expect(save).toHaveBeenCalledWith({ ...settings, auto_apply_ocr_corrections: true }),
    )
  })
})
