import { QueryClientProvider } from '@tanstack/react-query'
import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, vi } from 'vitest'

import { TypeBar } from '@/components/editor/TypeBar'
import { fontsKey, pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type CharStyle,
  type Edit,
  type Layer,
  type Page,
  type Text,
  type TextStyle,
  type Typed,
} from '@koharu/ipc/protocol'
import { TooltipProvider } from '@koharu/ui/components/tooltip'

import { comicFonts } from './fonts'

// The type bar over texts as a test letters them, for Booharu's tests of what it shows and sends.

/** A text layer `id` lettered as `fields` say, 24 high where they give no size. */
export function text(id: string, fields: Partial<Text> = {}): Layer {
  return {
    id,
    name: id,
    visible: true,
    locked: false,
    opacity: 1,
    blend: 'normal',
    attributes: {},
    kind: 'text',
    content: [{ insert: 'Hello' }],
    ...fields,
    style: { ...fields.style, chars: { size: 24, ...fields.style?.chars } },
  } as Layer
}

/** A text layer `id` whose characters are styled `chars`. */
export function styled(id: string, chars: CharStyle): Layer {
  return text(id, { style: { chars } })
}

/** The type bar once Koharu's ContextBar shows it, a turn after it mounts, when floating-ui has
 * placed it by the selection: hidden until then, nothing in it has a role. */
export async function shownBar(): Promise<HTMLElement> {
  const bar = await screen.findByTestId('type-bar')
  await waitFor(() => expect(bar).toBeVisible())
  return bar
}

/** A turn gone by, for a check that the bar sent nothing: its edits go as mutations, and its
 * commands through `call`, a turn after the press that sends them. */
export async function settled(): Promise<void> {
  await act(() => new Promise<void>((resolve) => setTimeout(resolve, 0)))
}

/** The TypeBar over `layers`, every one selected unless `selected` says which, with every command
 * answering null but the page's reads, once it shows; the edits it sends, which stay on their way
 * when `pending`; and, given `typing`, the canvas typing. */
export async function renderBar(
  layers: Layer[],
  {
    typing = null,
    pending = false,
    selected = layers.map((layer) => layer.id),
  }: { typing?: Typed | null; pending?: boolean; selected?: string[] } = {},
) {
  const edits: Edit[] = []
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  vi.spyOn(commands, 'getFontPreview').mockResolvedValue(new ArrayBuffer(0))
  vi.spyOn(commands, 'edit').mockImplementation((edit) => {
    edits.push(edit)
    return pending ? new Promise(() => undefined) : Promise.resolve(null as never)
  })
  const page: Page = { id: 'page', size: { width: 800, height: 1200 }, background: null, layers }
  // The page the canvas shows, as Koharu reads it again once one of the bar's edits settles: as
  // the test letters it, as the edits only go to the list.
  vi.spyOn(commands, 'getShownPage').mockResolvedValue(page.id)
  vi.spyOn(commands, 'query').mockImplementation(async (query) =>
    query.query === 'get_page' ? page : (null as never),
  )
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(fontsKey, comicFonts)
  useKoharuStore.setState({
    tool: 'select',
    selectedLayers: selected,
    selectionBounds: { x: 100, y: 100, width: 200, height: 80 },
    typing,
  })
  render(
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delay={0}>
        <TypeBar />
      </TooltipProvider>
    </QueryClientProvider>,
  )
  return { edits, bar: within(await shownBar()), user: userEvent.setup() }
}

/** One edit setting `style` on each of `layers`, as setStyle sends it to texts with no marks. */
export function restyled(layers: [string, TextStyle][]): Edit {
  return {
    label: 'set_style',
    ops: layers.map(([layer, style]) => ({ op: 'update_layer', layer, patch: { style } })),
  }
}
