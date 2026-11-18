'use client'

/** Why a preset name typed was refused, which the field it's typed in names as its description. */
export function NameError({ id, children }: { id: string; children: string }) {
  return (
    <p id={id} role='alert' className='text-[10px] leading-4 text-destructive'>
      {children}
    </p>
  )
}
