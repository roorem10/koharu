import { QueryClientProvider } from '@tanstack/react-query'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'

import { TypeBar } from '@/components/editor/TypeBar'
import { setWordRange } from '@/lib/booharu/words'
import { fontsKey, pageKey, queryClient } from '@/lib/queries'
import { useKoharuStore } from '@/lib/store'
import {
  commands,
  type CharStyle,
  type Edit,
  type FontFamily,
  type Page,
  type Span,
  type Typed,
} from '@koharu/ipc/protocol'

import { comicFonts } from './fonts'

/** A family with a light upright face and a semibold italic one, and no regular or bold face. */
const comic: FontFamily = {
  name: 'Comic',
  faces: [
    { post_script_name: 'Comic-Light', weight: 300, weight_range: null, italic: false },
    { post_script_name: 'Comic-SemiBoldItalic', weight: 600, weight_range: null, italic: true },
  ],
}

/** A page with one text layer, `text`, styled `chars`, whose content is `content`. */
function pageWith(chars: CharStyle, content: Span[] = [{ insert: 'Hello' }]): Page {
  return {
    id: 'page',
    size: { width: 800, height: 1200 },
    background: null,
    layers: [
      {
        id: 'text',
        name: 'Text',
        visible: true,
        locked: false,
        opacity: 1,
        blend: 'normal',
        attributes: {},
        kind: 'text',
        content,
        style: { chars: { size: 24, ...chars } },
      },
    ],
  }
}

/** The TypeBar over the text of `page`, selected, with every command answering null, the edits it
 * sends, and, given `typing`, the canvas typing into it. */
function renderBar(page: Page, typing: Typed | null = null) {
  const edits: Edit[] = []
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  vi.spyOn(commands, 'getFontPreview').mockResolvedValue(new ArrayBuffer(0))
  vi.spyOn(commands, 'edit').mockImplementation(async (edit) => {
    edits.push(edit)
    return null as never
  })
  queryClient.setQueryData(pageKey, page)
  queryClient.setQueryData(fontsKey, [...comicFonts, comic])
  useKoharuStore.setState({
    tool: 'select',
    selectedLayers: ['text'],
    selectionBounds: { x: 100, y: 100, width: 200, height: 80 },
    typing,
  })
  render(
    <QueryClientProvider client={queryClient}>
      <TypeBar />
    </QueryClientProvider>,
  )
  return { edits, bar: within(screen.getByTestId('type-bar')) }
}

/** The one edit setting `chars` on the text's style, as setStyle sends it to a text no word of
 * which has a mark of its own for those keys. */
function restyled(chars: CharStyle): Edit {
  return {
    label: 'set_style',
    ops: [{ op: 'update_layer', layer: 'text', patch: { style: { chars } } }],
  }
}

describe('Booharu italic and the font picker in the TypeBar', () => {
  it('turns italic on for a family with no italic face, sending only italic', async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(pageWith({ family: 'Wild Words', weight: 700 }))

    const italic = bar.getByRole('button', { name: 'Italic' })
    expect(italic).toBeEnabled()
    expect(italic).toHaveAttribute('aria-pressed', 'false')
    await user.click(italic)

    expect(edits).toEqual([restyled({ italic: true })])
  })

  it('shows italic on such a family as the text has it, and sends only italic off', async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(pageWith({ family: 'Wild Words', italic: true }))

    const italic = bar.getByRole('button', { name: 'Italic' })
    expect(italic).toHaveAttribute('aria-pressed', 'true')
    await user.click(italic)

    expect(edits).toEqual([restyled({ italic: false })])
  })

  it('styles the characters typed with italic alone', async () => {
    const user = userEvent.setup()
    const { bar } = renderBar(pageWith({ family: 'Wild Words' }), {
      layer: 'text',
      chars: { family: 'Wild Words', italic: true },
      mixed: [],
    })

    await user.click(bar.getByRole('button', { name: 'Italic' }))

    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({ italic: false })
  })

  it('changes only the family, so bold words stay bold and the box stays italic (D22)', async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(
      pageWith({ family: 'Komika Text', italic: true }, [
        { insert: 'Hello ' },
        { insert: 'world', attributes: { weight: 700, family: 'Anime Ace' } },
      ]),
    )

    await user.click(bar.getByTestId('type-font-picker'))
    await user.click(await screen.findByRole('option', { name: 'Wild Words' }))

    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          {
            op: 'update_layer',
            layer: 'text',
            patch: { style: { chars: { family: 'Wild Words' } } },
          },
          {
            op: 'set_rich_text',
            layer: 'text',
            spans: [
              { insert: 'Hello ', attributes: {} },
              { insert: 'world', attributes: { weight: 700 } },
            ],
          },
        ],
      },
    ])
  })
})

describe('Booharu weights and B in the TypeBar', () => {
  it("lists the family's weights of either style with regular, bold and the one shown", async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(pageWith({ family: 'Comic', weight: 550 }))

    const weights = bar.getByRole('combobox', { name: 'Font weight' })
    expect(weights).toHaveTextContent('550')
    await user.click(weights)
    const options = await screen.findAllByRole('option')
    expect(options.map((option) => option.textContent)).toEqual(['300', '400', '550', '600', '700'])

    // The weight picked is the weight sent, with nothing else, never snapped to a face.
    await user.click(screen.getByRole('option', { name: '400' }))
    await waitFor(() => expect(edits).toEqual([restyled({ weight: 400 })]))
  })

  it('shows B pressed for a bold text and makes it regular', async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(pageWith({ family: 'Comic', weight: 700 }))

    const bold = bar.getByRole('button', { name: 'Bold' })
    expect(bold).toHaveAttribute('aria-pressed', 'true')
    await user.click(bold)

    expect(edits).toEqual([restyled({ weight: 400 })])
  })

  it('shows B off for a light or regular text and makes it bold', async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(pageWith({ family: 'Comic', weight: 300 }))

    const bold = bar.getByRole('button', { name: 'Bold' })
    expect(bold).toBeEnabled()
    expect(bold).toHaveAttribute('aria-pressed', 'false')
    await user.click(bold)

    expect(edits).toEqual([restyled({ weight: 700 })])
  })

  it('makes words of mixed weights bold, every word', async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(
      pageWith({ family: 'Comic' }, [
        { insert: 'Hello ', attributes: { weight: 800 } },
        { insert: 'world' },
      ]),
    )

    const bold = bar.getByRole('button', { name: 'Bold' })
    expect(bold).toHaveAttribute('aria-pressed', 'false')
    await user.click(bold)

    expect(edits).toEqual([
      {
        label: 'set_style',
        ops: [
          { op: 'update_layer', layer: 'text', patch: { style: { chars: { weight: 700 } } } },
          {
            op: 'set_rich_text',
            layer: 'text',
            spans: [
              { insert: 'Hello ', attributes: {} },
              { insert: 'world', attributes: {} },
            ],
          },
        ],
      },
    ])
  })

  it("gives a light text's own weight back when B goes off while typing", async () => {
    const user = userEvent.setup()
    const { bar } = renderBar(pageWith({ family: 'Comic', weight: 300 }), {
      layer: 'text',
      chars: { family: 'Comic', weight: 700 },
      mixed: [],
    })

    const bold = bar.getByRole('button', { name: 'Bold' })
    expect(bold).toHaveAttribute('aria-pressed', 'true')
    await user.click(bold)

    expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith({ weight: 300 })
  })

  it('gives regular back while typing on a regular or bold text, and bold on', async () => {
    const user = userEvent.setup()
    const { bar } = renderBar(pageWith({ family: 'Comic', weight: 700 }), {
      layer: 'text',
      chars: { family: 'Comic', weight: 700 },
      mixed: [],
    })
    await user.click(bar.getByRole('button', { name: 'Bold' }))
    expect(commands.styleTyping).toHaveBeenLastCalledWith({ weight: 400 })

    useKoharuStore.setState({ typing: { layer: 'text', chars: {}, mixed: ['weight'] } })
    await waitFor(() =>
      expect(bar.getByRole('button', { name: 'Bold' })).toHaveAttribute('aria-pressed', 'false'),
    )
    await user.click(bar.getByRole('button', { name: 'Bold' }))
    expect(commands.styleTyping).toHaveBeenLastCalledWith({ weight: 700 })
  })
})

describe('Booharu emphasis star in the TypeBar', () => {
  it('is disabled on a whole box, and on a new text being typed', () => {
    const { bar } = renderBar(pageWith({ family: 'Comic' }))
    expect(bar.getByRole('button', { name: 'Emphasis' })).toBeDisabled()

    useKoharuStore.setState({ typing: { layer: null, chars: {}, mixed: [] } })
    expect(bar.getByRole('button', { name: 'Emphasis' })).toBeDisabled()
  })

  it("gives the characters typed their category's emphasis", async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(pageWith({ family: 'Comic' }), {
      layer: 'text',
      chars: { family: 'Comic' },
      mixed: [],
    })
    const patch: CharStyle = { family: null, weight: 700, size: 27.6 }
    const emphasis = vi.spyOn(commands, 'booharuEmphasisPatch').mockResolvedValue(patch)

    const star = bar.getByRole('button', { name: 'Emphasis' })
    expect(star).toBeEnabled()
    await user.click(star)

    expect(emphasis).toHaveBeenCalledExactlyOnceWith('text')
    await waitFor(() => expect(commands.styleTyping).toHaveBeenCalledExactlyOnceWith(patch))
    expect(commands.booharuStyleWords).not.toHaveBeenCalled()
    expect(edits).toEqual([])
  })

  it('sends nothing to a text the canvas stopped typing into meanwhile', async () => {
    const user = userEvent.setup()
    const { bar } = renderBar(pageWith({ family: 'Comic' }), {
      layer: 'text',
      chars: {},
      mixed: [],
    })
    let answer: (patch: CharStyle) => void = () => {}
    vi.spyOn(commands, 'booharuEmphasisPatch').mockImplementation(
      () => new Promise((resolve) => (answer = resolve)),
    )

    await user.click(bar.getByRole('button', { name: 'Emphasis' }))
    useKoharuStore.setState({ typing: { layer: 'other', chars: {}, mixed: [] } })
    answer({ weight: 700 })

    await new Promise((resolve) => setTimeout(resolve, 0))
    expect(commands.styleTyping).not.toHaveBeenCalled()
  })

  it('stresses the words chosen in Story in the text the bar shows, and no others', async () => {
    const user = userEvent.setup()
    const { edits, bar } = renderBar(pageWith({ family: 'Comic' }, [{ insert: 'Hello world' }]))

    setWordRange({ layer: 'other', start: 0, end: 5, text: 'Hello' })
    await waitFor(() => expect(bar.getByRole('button', { name: 'Emphasis' })).toBeDisabled())

    setWordRange({ layer: 'text', start: 6, end: 11, text: 'Hello world' })
    await waitFor(() => expect(bar.getByRole('button', { name: 'Emphasis' })).toBeEnabled())
    await user.click(bar.getByRole('button', { name: 'Emphasis' }))

    expect(commands.booharuStyleWords).toHaveBeenCalledExactlyOnceWith(
      'text',
      6,
      11,
      'Hello world',
      { emphasis: true },
    )
    expect(commands.styleTyping).not.toHaveBeenCalled()
    expect(edits).toEqual([])
  })
})
