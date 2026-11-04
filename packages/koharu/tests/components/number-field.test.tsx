import { act, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'

function Field({
  change,
  commit,
}: {
  change: (value: number | null, details: unknown) => void
  commit: (value: number | null, details: unknown) => void
}) {
  const [value, setValue] = useState<number | null>(10)
  return (
    <NumberField
      value={value}
      step={0.5}
      min={0}
      max={20}
      onValueChange={(next, details) => {
        setValue(next)
        change(next, details)
      }}
      onValueCommitted={commit}
    >
      <NumberFieldGroup>
        <NumberFieldInput aria-label='Value' />
      </NumberFieldGroup>
    </NumberField>
  )
}

afterEach(() => vi.useRealTimers())

describe('number field wheel', () => {
  it('keeps ticks live and batches one change and commit, including Shift at five steps', () => {
    vi.useFakeTimers()
    const change = vi.fn()
    const commit = vi.fn()
    render(<Field change={change} commit={commit} />)
    const input = screen.getByRole('textbox', { name: 'Value' })
    act(() => input.focus())
    const tick = new WheelEvent('wheel', { deltaY: -100, bubbles: true, cancelable: true })
    act(() => input.dispatchEvent(tick))
    expect(input).toHaveValue('10.5')
    expect(tick.defaultPrevented).toBe(true)
    fireEvent.wheel(input, { deltaY: -100, shiftKey: true })
    expect(input).toHaveValue('13')
    act(() => vi.advanceTimersByTime(299))
    expect(change).not.toHaveBeenCalled()
    expect(commit).not.toHaveBeenCalled()
    act(() => vi.advanceTimersByTime(1))
    expect(change).toHaveBeenCalledExactlyOnceWith(13, expect.objectContaining({ reason: 'wheel' }))
    expect(commit).toHaveBeenCalledExactlyOnceWith(13, expect.objectContaining({ reason: 'wheel' }))
    fireEvent.blur(input)
    expect(commit).toHaveBeenCalledTimes(1)
  })

  it('ignores an unfocused wheel and flushes a pending burst once on unmount', () => {
    vi.useFakeTimers()
    const change = vi.fn()
    const commit = vi.fn()
    const view = render(<Field change={change} commit={commit} />)
    const input = screen.getByRole('textbox', { name: 'Value' })
    fireEvent.wheel(input, { deltaY: -100 })
    act(() => vi.advanceTimersByTime(400))
    expect(change).not.toHaveBeenCalled()
    act(() => input.focus())
    fireEvent.wheel(input, { deltaY: -100 })
    view.unmount()
    act(() => vi.advanceTimersByTime(400))
    expect(change).toHaveBeenCalledExactlyOnceWith(10.5, expect.anything())
    expect(commit).toHaveBeenCalledTimes(1)
  })
})
