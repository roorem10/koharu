import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen } from '@testing-library/react'
import type { ComponentProps } from 'react'
import { vi } from 'vitest'

import { FontPicker } from '@/components/controls/FontPicker'
import { queryClient } from '@/lib/queries'
import { commands, type FontFamily } from '@koharu/ipc/protocol'

/** Installed families by name, each with one regular face, its PostScript name without spaces. */
export function fontFamilies(...names: string[]): FontFamily[] {
  return names.map((name) => ({
    name,
    faces: [
      {
        post_script_name: name.replaceAll(' ', ''),
        weight: 400,
        weight_range: null,
        italic: false,
      },
    ],
  }))
}

/** Four comic fonts, in name order. */
export const comicFonts = fontFamilies('Anime Ace', 'CCMeanwhile', 'Komika Text', 'Wild Words')

/** Koharu's font picker, named Font, choosing among the comic fonts unless `props` say otherwise;
 * every family's preview is a blank image. */
export function renderPicker(props: Partial<ComponentProps<typeof FontPicker>> = {}) {
  vi.spyOn(commands, 'getFontPreview').mockResolvedValue(new ArrayBuffer(0))
  const onChange = vi.fn()
  render(
    <QueryClientProvider client={queryClient}>
      <FontPicker value='' families={comicFonts} ariaLabel='Font' onChange={onChange} {...props} />
    </QueryClientProvider>,
  )
  return { onChange }
}

/** The search line, a combobox over the fonts. */
export function search(): HTMLElement {
  return screen.getByRole('combobox', { name: 'Search fonts' })
}

/** The font the arrows are on: the option the search line names as its active descendant. */
export function highlighted(): HTMLElement | null {
  const id = search().getAttribute('aria-activedescendant')
  return id ? document.getElementById(id) : null
}
