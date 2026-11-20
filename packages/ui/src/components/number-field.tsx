'use client'

import { NumberField as NumberFieldPrimitive } from '@base-ui/react/number-field'
import * as React from 'react'

import { cn } from '@koharu/ui/lib/utils'

/** Booharu: Base UI's root, with the mouse wheel stepping a focused number box under the pointer,
 * as Booharu 0.0.4 and Photoshop step it (D29), every field alike. Base UI already turns the wheel
 * only over a focused input and leaves Ctrl+wheel to zoom. The wheel is on only while the field
 * shows a number, as Base UI would seed an empty one, Auto, Mixed or an unset value, with its
 * minimum. Shift steps five steps, on the wheel, the arrow keys and the steppers alike. The ticks
 * stay in the field, which shows each, and its callers hear one change, then one commit, 300 ms
 * after the last, so a burst is one write and one undo step; any other change or commit, the focus
 * or the pointer leaving the field, and unmounting send a burst first. A caller's `allowWheelScrub`
 * and `largeStep` win. */
const NumberField = React.forwardRef<HTMLDivElement, NumberFieldPrimitive.Root.Props>(
  function NumberField(
    {
      value,
      defaultValue = null,
      step = 1,
      largeStep,
      allowWheelScrub,
      onValueChange,
      onValueCommitted,
      onBlur,
      onPointerLeave,
      ...props
    },
    ref,
  ) {
    const [uncontrolled, setUncontrolled] = React.useState<number | null>(defaultValue)
    // The burst's value while it waits, which the field shows in place of `value`.
    const [wheelValue, setWheelValue] = React.useState<{ value: number | null } | null>(null)
    const pending = React.useRef<{
      value: number | null
      change: NumberFieldPrimitive.Root.ChangeEventDetails
      commit?: NumberFieldPrimitive.Root.CommitEventDetails
    } | null>(null)
    // The value a burst last committed, so a commit of that value right after it isn't sent again,
    // as Base UI's on releasing a stepper press that changed nothing.
    const flushed = React.useRef<{ value: number | null } | null>(null)
    const timer = React.useRef<ReturnType<typeof setTimeout> | null>(null)
    // The latest callbacks, which a burst sent later calls.
    const latest = React.useRef({ onValueChange, onValueCommitted })

    React.useEffect(() => {
      latest.current = { onValueChange, onValueCommitted }
    })

    const flush = React.useCallback(() => {
      if (timer.current !== null) clearTimeout(timer.current)
      timer.current = null
      const next = pending.current
      pending.current = null
      if (!next) return
      latest.current.onValueChange?.(next.value, next.change)
      if (!next.commit) return
      flushed.current = { value: next.value }
      latest.current.onValueCommitted?.(next.value, next.commit)
    }, [])

    React.useEffect(() => () => flush(), [flush])
    React.useEffect(() => setWheelValue(null), [value])

    const shown = wheelValue ? wheelValue.value : value === undefined ? uncontrolled : value
    return (
      <NumberFieldPrimitive.Root
        {...props}
        ref={ref}
        value={shown}
        step={step}
        largeStep={largeStep ?? (typeof step === 'number' ? step * 5 : 5)}
        allowWheelScrub={allowWheelScrub ?? typeof shown === 'number'}
        onValueChange={(next, details) => {
          if (details.reason !== 'wheel') {
            flush()
            flushed.current = null
            setWheelValue(null)
            setUncontrolled(next)
            onValueChange?.(next, details)
            return
          }
          setWheelValue({ value: next })
          setUncontrolled(next)
          pending.current = { value: next, change: details }
          if (timer.current !== null) clearTimeout(timer.current)
          timer.current = setTimeout(flush, 300)
        }}
        onValueCommitted={(next, details) => {
          // Base UI commits each tick: the burst's one commit goes when it does.
          if (details.reason === 'wheel') {
            if (pending.current) pending.current.commit = details
            return
          }
          flush()
          const sent = flushed.current
          flushed.current = null
          if (!sent || sent.value !== next) onValueCommitted?.(next, details)
        }}
        onBlur={(event) => {
          flush()
          onBlur?.(event)
        }}
        onPointerLeave={(event) => {
          flush()
          onPointerLeave?.(event)
        }}
      />
    )
  },
)

function NumberFieldGroup({ className, ...props }: NumberFieldPrimitive.Group.Props) {
  return (
    <NumberFieldPrimitive.Group
      data-slot='number-field-group'
      className={cn(
        'flex h-6 w-full min-w-0 items-center overflow-hidden rounded-md border border-input bg-transparent transition-colors focus-within:border-ring focus-within:ring-2 focus-within:ring-ring/30 dark:bg-input/30 data-disabled:cursor-not-allowed data-disabled:opacity-50',
        className,
      )}
      {...props}
    />
  )
}

function NumberFieldInput({ className, ...props }: NumberFieldPrimitive.Input.Props) {
  return (
    <NumberFieldPrimitive.Input
      data-slot='number-field-input'
      className={cn(
        'h-full min-w-0 flex-1 bg-transparent px-1 text-center text-[11px] tabular-nums outline-none selection:bg-primary selection:text-primary-foreground',
        className,
      )}
      {...props}
    />
  )
}

function NumberFieldDecrement({ className, ...props }: NumberFieldPrimitive.Decrement.Props) {
  return (
    <NumberFieldPrimitive.Decrement
      data-slot='number-field-decrement'
      className={cn(
        'grid size-6 shrink-0 place-items-center border-r border-input text-muted-foreground transition-colors outline-none hover:bg-muted hover:text-foreground focus-visible:bg-muted focus-visible:text-foreground disabled:pointer-events-none disabled:opacity-40 [&_svg:not([class*="size-"])]:size-3',
        className,
      )}
      {...props}
    />
  )
}

function NumberFieldIncrement({ className, ...props }: NumberFieldPrimitive.Increment.Props) {
  return (
    <NumberFieldPrimitive.Increment
      data-slot='number-field-increment'
      className={cn(
        'grid size-6 shrink-0 place-items-center border-l border-input text-muted-foreground transition-colors outline-none hover:bg-muted hover:text-foreground focus-visible:bg-muted focus-visible:text-foreground disabled:pointer-events-none disabled:opacity-40 [&_svg:not([class*="size-"])]:size-3',
        className,
      )}
      {...props}
    />
  )
}

export {
  NumberField,
  NumberFieldDecrement,
  NumberFieldGroup,
  NumberFieldIncrement,
  NumberFieldInput,
}
