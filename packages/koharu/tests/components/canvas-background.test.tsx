import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

import { canvasBackgroundsKey } from '@/components/preferences/canvas-background'
import { CanvasBackgroundPreferences } from '@/components/preferences/CanvasBackgroundPreferences'
import { queryClient } from '@/lib/queries'
import translations from '@/public/locales/en-US/translation.json'
import { commands, type CanvasBackgroundColors } from '@koharu/bridge/protocol'

vi.mock('@/components/controls/ColorWell', () => ({
  ColorWell: ({
    label,
    value,
    disabled,
    onChange,
  }: {
    label: string
    value: string
    disabled: boolean
    onChange: (color: string) => void
  }) => (
    <input
      type='color'
      aria-label={label}
      value={value}
      disabled={disabled}
      onChange={(event) => onChange(event.currentTarget.value)}
    />
  ),
}))

const labels = translations.settings.appearance
let saved: CanvasBackgroundColors

beforeEach(() => {
  saved = { light: [255, 255, 255], dark: [0, 0, 0] }
  vi.spyOn(commands, 'getCanvasBackgrounds').mockImplementation(async () => structuredClone(saved))
  vi.spyOn(commands, 'saveCanvasBackground').mockImplementation(async (theme, color) => {
    saved = { ...saved, [theme]: color }
    return structuredClone(saved)
  })
})

describe('canvas background preferences', () => {
  it('commits numeric colors for each theme and reloads the persisted values', async () => {
    const first = render(<CanvasBackgroundPreferences />)
    const light = await screen.findByLabelText(labels.lightCanvasBackground)
    fireEvent.change(light, { target: { value: '#abcdef' } })
    await waitFor(() =>
      expect(commands.saveCanvasBackground).toHaveBeenCalledWith('light', [171, 205, 239]),
    )
    const dark = screen.getByLabelText(labels.darkCanvasBackground)
    await waitFor(() => expect(dark).not.toBeDisabled())
    fireEvent.change(dark, { target: { value: '#123456' } })
    await waitFor(() => expect(saved).toEqual({ light: [171, 205, 239], dark: [18, 52, 86] }))
    first.unmount()
    queryClient.removeQueries({ queryKey: canvasBackgroundsKey })
    render(<CanvasBackgroundPreferences />)
    expect(await screen.findByLabelText(labels.lightCanvasBackground)).toHaveValue('#abcdef')
    expect(screen.getByLabelText(labels.darkCanvasBackground)).toHaveValue('#123456')
  })

  it('waits for a pending save before accepting another color', async () => {
    let resolve!: (colors: CanvasBackgroundColors) => void
    const pending = new Promise<CanvasBackgroundColors>((done) => {
      resolve = done
    })
    vi.mocked(commands.saveCanvasBackground).mockReturnValueOnce(pending)
    render(<CanvasBackgroundPreferences />)
    const light = await screen.findByLabelText(labels.lightCanvasBackground)
    const dark = screen.getByLabelText(labels.darkCanvasBackground)
    fireEvent.change(light, { target: { value: '#abcdef' } })
    await waitFor(() => {
      expect(light).toBeDisabled()
      expect(dark).toBeDisabled()
    })
    await act(async () => resolve({ light: [171, 205, 239], dark: [0, 0, 0] }))
    await waitFor(() => expect(dark).not.toBeDisabled())
    expect(queryClient.getQueryData(canvasBackgroundsKey)).toEqual({
      light: [171, 205, 239],
      dark: [0, 0, 0],
    })
  })
})
