'use client'

import type { ComponentProps } from 'react'

import { Tooltip, TooltipContent, TooltipTrigger } from '@koharu/ui/components/tooltip'

/** A right pane tab as it's drawn, given to Koharu's `TabsTrigger` as its `render`: the tab, with
 * `tab`, the props the trigger gives it, named `name` in a tooltip while the tabs show their icons
 * alone (`alone`), each label left to screen readers. The page draws the tooltip, as it draws
 * Koharu's icon buttons': the desktop draws the interface off screen, where a `title` never
 * shows. */
export function TabTooltip({
  name,
  alone,
  ...tab
}: ComponentProps<typeof TooltipTrigger> & { name: string; alone: boolean }) {
  return (
    <Tooltip disabled={!alone}>
      <TooltipTrigger {...tab} />
      <TooltipContent side='bottom'>{name}</TooltipContent>
    </Tooltip>
  )
}
