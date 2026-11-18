'use client'

import { Check } from 'lucide-react'

/** The leading row of Koharu's font list, `top` pixels down it, given a mount's `noneLabel`: an
 * option labelled `label` that stands for no family, checked when `chosen`, the value being empty,
 * and marked when `active`, the arrows being on it. Moving the pointer over it, or the focus to it,
 * marks it, as moving either to a family's row does, and picking it calls `onSelect`. */
export function NoneOption({
  id,
  label,
  top,
  chosen,
  active,
  onActive,
  onSelect,
}: {
  id: string
  label: string
  top: number
  chosen: boolean
  active: boolean
  onActive: () => void
  onSelect: () => void
}) {
  return (
    <button
      type='button'
      role='option'
      aria-label={label} // Named as a family's option is.
      id={id}
      aria-selected={chosen}
      data-active={active}
      className='absolute inset-x-0 top-0 flex h-9.5 w-full items-center gap-2 px-2 text-left text-[11px] focus-visible:outline-none data-[active=true]:bg-accent data-[active=true]:text-accent-foreground'
      style={{ transform: `translateY(${top}px)` }}
      // Only the pointer's own moves mark it, as for a family's row.
      onPointerMove={(event) => {
        if (event.movementX || event.movementY) onActive()
      }}
      onFocus={onActive}
      onClick={onSelect}
    >
      <span className='min-w-0 flex-1 truncate'>{label}</span>
      {chosen && <Check className='size-3.5 shrink-0' />}
    </button>
  )
}
