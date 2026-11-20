import { act, fireEvent, render, screen } from '@testing-library/react'
import { useState } from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { NumberField, NumberFieldGroup, NumberFieldInput } from '@koharu/ui/components/number-field'

type Callback = (value: number | null, details: unknown) => void

/** A number box holding its own value, from `start`, at a step of 0.5 from 0 to 20, telling
 * `change` and `commit` what Booharu's NumberField tells its callers. */
function Field({
  change,
  commit,
  start = 10,
  allowWheelScrub,
}: {
  change: Callback
  commit: Callback
  start?: number | null
  allowWheelScrub?: boolean
}) {
  const [value, setValue] = useState<number | null>(start)
  return (
    <NumberField
      data-testid='field'
      value={value}
      step={0.5}
      min={0}
      max={20}
      allowWheelScrub={allowWheelScrub}
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

/** One turn of the wheel up on `input`, with Shift when `shift`, and the event. */
function wheelUp(input: HTMLElement, shift = false): WheelEvent {
  const tick = new WheelEvent('wheel', {
    deltaY: -100,
    shiftKey: shift,
    bubbles: true,
    cancelable: true,
  })
  act(() => void input.dispatchEvent(tick))
  return tick
}

function renderField(props: Partial<Parameters<typeof Field>[0]> = {}) {
  vi.useFakeTimers()
  const change = vi.fn<Callback>()
  const commit = vi.fn<Callback>()
  const view = render(<Field change={change} commit={commit} {...props} />)
  const input = screen.getByRole('textbox', { name: 'Value' })
  return { change, commit, view, input }
}

afterEach(() => vi.useRealTimers())

describe('Booharu number box wheel (D29)', () => {
  it('shows each tick and sends one change, then one commit, 300 ms after the last', () => {
    const { change, commit, input } = renderField()
    act(() => input.focus())

    const tick = wheelUp(input)
    expect(input).toHaveValue('10.5')
    expect(tick.defaultPrevented).toBe(true)
    // Shift steps five steps.
    wheelUp(input, true)
    expect(input).toHaveValue('13')
    act(() => vi.advanceTimersByTime(299))
    expect(change).not.toHaveBeenCalled()
    expect(commit).not.toHaveBeenCalled()

    act(() => vi.advanceTimersByTime(1))
    expect(change).toHaveBeenCalledExactlyOnceWith(13, expect.objectContaining({ reason: 'wheel' }))
    expect(commit).toHaveBeenCalledExactlyOnceWith(13, expect.objectContaining({ reason: 'wheel' }))
    expect(change.mock.invocationCallOrder[0]).toBeLessThan(commit.mock.invocationCallOrder[0]!)

    // Leaving the box commits nothing more.
    fireEvent.blur(input)
    expect(commit).toHaveBeenCalledTimes(1)
    expect(input).toHaveValue('13')
  })

  it('ignores the wheel on a box without the focus, or showing no number', () => {
    const { change, input } = renderField()
    fireEvent.wheel(input, { deltaY: -100 })
    act(() => vi.advanceTimersByTime(400))
    expect(change).not.toHaveBeenCalled()
    expect(input).toHaveValue('10')
  })

  it('ignores the wheel on a box showing no number, which Base UI would seed', () => {
    const { change, commit, input } = renderField({ start: null })
    act(() => input.focus())

    const tick = wheelUp(input)
    act(() => vi.advanceTimersByTime(400))
    expect(tick.defaultPrevented).toBe(false)
    expect(input).toHaveValue('')
    expect(change).not.toHaveBeenCalled()
    expect(commit).not.toHaveBeenCalled()
  })

  it("leaves the wheel off when the caller turns it off, and steps five steps with Shift's keys", () => {
    const { change, input } = renderField({ allowWheelScrub: false })
    act(() => input.focus())

    wheelUp(input)
    act(() => vi.advanceTimersByTime(400))
    expect(change).not.toHaveBeenCalled()

    fireEvent.keyDown(input, { key: 'ArrowUp', shiftKey: true })
    expect(change).toHaveBeenCalledExactlyOnceWith(12.5, expect.anything())
  })

  it('sends a waiting burst at once as the focus or the pointer leaves, or the box goes', () => {
    const { change, commit, input, view } = renderField()
    act(() => input.focus())

    wheelUp(input)
    fireEvent.blur(input)
    expect(change).toHaveBeenCalledExactlyOnceWith(10.5, expect.anything())
    expect(commit).toHaveBeenCalledExactlyOnceWith(10.5, expect.anything())

    act(() => input.focus())
    wheelUp(input)
    fireEvent.pointerLeave(screen.getByTestId('field'))
    expect(change).toHaveBeenLastCalledWith(11, expect.anything())
    expect(commit).toHaveBeenLastCalledWith(11, expect.anything())

    wheelUp(input)
    view.unmount()
    expect(change).toHaveBeenCalledTimes(3)
    expect(commit).toHaveBeenCalledTimes(3)
    expect(commit).toHaveBeenLastCalledWith(11.5, expect.anything())
    act(() => vi.advanceTimersByTime(400))
    expect(commit).toHaveBeenCalledTimes(3)
  })

  it('sends a waiting burst before a change by the keys, to the latest callbacks', () => {
    const { change, commit, input, view } = renderField()
    act(() => input.focus())
    wheelUp(input)

    // The caller's callbacks changed during the burst: the new ones hear it.
    const later = vi.fn<Callback>()
    const laterCommit = vi.fn<Callback>()
    view.rerender(<Field change={later} commit={laterCommit} />)
    fireEvent.keyDown(input, { key: 'ArrowUp' })

    expect(change).not.toHaveBeenCalled()
    expect(commit).not.toHaveBeenCalled()
    expect(later.mock.calls.map(([value]) => value)).toEqual([10.5, 11])
    expect(laterCommit.mock.calls.map(([value]) => value)).toEqual([10.5, 11])
    act(() => vi.advanceTimersByTime(400))
    expect(later).toHaveBeenCalledTimes(2)
  })

  it('shows the burst until the value given changes, as when a write is still on its way', () => {
    vi.useFakeTimers()
    const change = vi.fn<Callback>()
    const view = render(
      <NumberField value={10} step={0.5} onValueChange={change}>
        <NumberFieldGroup>
          <NumberFieldInput aria-label='Value' />
        </NumberFieldGroup>
      </NumberField>,
    )
    const input = screen.getByRole('textbox', { name: 'Value' })
    act(() => input.focus())

    wheelUp(input)
    act(() => vi.advanceTimersByTime(300))
    expect(change).toHaveBeenCalledExactlyOnceWith(10.5, expect.anything())
    expect(input).toHaveValue('10.5')

    view.rerender(
      <NumberField value={12} step={0.5} onValueChange={change}>
        <NumberFieldGroup>
          <NumberFieldInput aria-label='Value' />
        </NumberFieldGroup>
      </NumberField>,
    )
    expect(input).toHaveValue('12')
  })
})
