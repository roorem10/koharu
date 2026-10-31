'use client'

import { NumberField as NumberFieldPrimitive } from '@base-ui/react/number-field'
import * as React from 'react'

import { cn } from '@koharu/ui/lib/utils'

const NumberField = React.forwardRef<HTMLDivElement, NumberFieldPrimitive.Root.Props>(
  function NumberField(
    { value, defaultValue = null, step = 1, largeStep, onValueChange, onValueCommitted, ...props },
    ref,
  ) {
    const [uncontrolled, setUncontrolled] = React.useState(defaultValue)
    const [wheelValue, setWheelValue] = React.useState<{ value: number | null } | null>(null)
    const pending = React.useRef<{
      value: number | null
      change: NumberFieldPrimitive.Root.ChangeEventDetails
      commit?: NumberFieldPrimitive.Root.CommitEventDetails
      onChange: typeof onValueChange
      onCommit: typeof onValueCommitted
    } | null>(null)
    const timer = React.useRef<ReturnType<typeof setTimeout> | null>(null)

    const flush = React.useCallback(() => {
      if (timer.current !== null) clearTimeout(timer.current)
      timer.current = null
      const next = pending.current
      pending.current = null
      if (!next) return
      next.onChange?.(next.value, next.change)
      if (next.commit) next.onCommit?.(next.value, next.commit)
    }, [])

    React.useEffect(() => () => flush(), [flush])
    React.useEffect(() => setWheelValue(null), [value])

    return (
      <NumberFieldPrimitive.Root
        {...props}
        ref={ref}
        value={wheelValue ? wheelValue.value : value === undefined ? uncontrolled : value}
        step={step}
        largeStep={largeStep ?? (typeof step === 'number' ? step * 5 : 5)}
        allowWheelScrub
        onValueChange={(next, details) => {
          if (details.reason !== 'wheel') {
            flush()
            setWheelValue(null)
            setUncontrolled(next)
            onValueChange?.(next, details)
            return
          }
          // Keep ticks local: expensive consumers see only the settled value.
          setWheelValue({ value: next })
          setUncontrolled(next)
          pending.current = {
            value: next,
            change: details,
            onChange: onValueChange,
            onCommit: onValueCommitted,
          }
          if (timer.current !== null) clearTimeout(timer.current)
          timer.current = setTimeout(flush, 300)
        }}
        onValueCommitted={(next, details) => {
          if (details.reason === 'wheel') {
            if (pending.current) pending.current.commit = details
          } else {
            const wheel = pending.current
            flush()
            if (!wheel || wheel.value !== next) onValueCommitted?.(next, details)
          }
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
