import { screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'

import { LayersPanel } from '@/components/editor/LayersPanel'
import { StoryPanel } from '@/components/editor/StoryPanel'

import {
  categoryNames,
  categorySearch,
  choicesListed,
  hint,
  letteredDesktop,
  mount,
} from './lettered'

/** The category tag of the Story row whose text reads `name`. */
function tagOf(name: string) {
  return screen.findByRole('combobox', { name: new RegExp(`^Lettering category for ${name}:`) })
}

describe("Booharu's lettering category in Story and Layers", () => {
  it('tags each Story row with its category, and changes it in two clicks', async () => {
    const { setCategory } = letteredDesktop()
    const user = userEvent.setup()
    mount(<StoryPanel />)

    // The category each block is lettered in, marked when the user picked it; a text layer has none.
    const tag = await tagOf('lo')
    expect(tag).toHaveAccessibleName('Lettering category for lo: Shout (Set by hand)')
    expect(tag).toHaveTextContent('Shout')
    expect(tag).toHaveAttribute('data-hand', 'true')
    const hel = await tagOf('Hel')
    expect(hel).toHaveAccessibleName(
      "Lettering category for Hel: Normal Dialogue (From its bubble's shape)",
    )
    expect(hel).toHaveAttribute('data-hand', 'false')
    expect(
      screen
        .getAllByRole('combobox', { name: /^Lettering category for/ })
        .map((each) => each.textContent),
    ).toEqual([
      'Normal Dialogue',
      'Shout',
      'SFX',
      'Whisper',
      'Narration Box',
      'Narration Box',
      'Floating',
      'Small Text',
    ])

    // Its tooltip says why, and the bubble's shape.
    await user.hover(hel)
    expect(await screen.findByText("From its bubble's shape")).toBeInTheDocument()
    expect(screen.getByText('Bubble shape: Round')).toBeInTheDocument()
    await user.unhover(hel)

    // A click opens Automatic and every category, the user's pick marked; a second click picks.
    await user.click(tag)
    await categorySearch()
    expect(choicesListed()).toEqual(['Automatic', ...categoryNames])
    expect(screen.getByRole('option', { name: 'Shout' })).toHaveAttribute('aria-selected', 'true')
    await user.click(screen.getByRole('option', { name: 'Whisper' }))
    await waitFor(() => expect(setCategory).toHaveBeenCalledExactlyOnceWith(['second'], 'whisper'))

    // Typing finds a category, and picking the one already chosen picks it again.
    await user.click(tag)
    await categorySearch()
    await user.keyboard('shou')
    expect(choicesListed()).toEqual(['Shout', 'Weak/Panic Shout'])
    await user.keyboard('{Enter}')
    await waitFor(() => expect(setCategory).toHaveBeenLastCalledWith(['second'], 'shout'))

    // Automatic, the choice of a block the agent labelled, clears the label: no category.
    await user.click(await tagOf('Hi'))
    await categorySearch()
    expect(screen.getByRole('option', { name: 'Automatic' })).toHaveAttribute(
      'aria-selected',
      'true',
    )
    await user.click(screen.getByRole('option', { name: 'Automatic' }))
    await waitFor(() => expect(setCategory).toHaveBeenLastCalledWith(['alone'], null))
    expect(setCategory).toHaveBeenCalledTimes(3)
  })

  it('opens a locked block’s picker only to read, saying why', async () => {
    const { setCategory } = letteredDesktop()
    const user = userEvent.setup()
    mount(<StoryPanel />)

    await user.click(await tagOf('Locked'))
    await categorySearch()
    expect(
      screen.getByText(
        "This text, or a layer it's in, is locked. Unlock it in Layers to change its category.",
      ),
    ).toBeInTheDocument()
    await user.click(screen.getByRole('option', { name: 'Shout' }))
    expect(setCategory).not.toHaveBeenCalled()
  })

  it('says under the list that fonts follow categories only with Pick fonts automatically on', async () => {
    letteredDesktop(false)
    const user = userEvent.setup()
    mount(<StoryPanel />)

    await user.click(await tagOf('Hel'))
    await categorySearch()
    const line = await screen.findByText(hint)
    expect(within(line).getByRole('button', { name: 'Pick fonts automatically' })).toBeVisible()
  })

  it('names each block row in Layers by its category', async () => {
    letteredDesktop()
    mount(<LayersPanel />)

    // Each layer of the fixture is named by its id.
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Edit first' })).toHaveTextContent(
        'firstNormal Dialogue',
      ),
    )
    expect(screen.getByRole('button', { name: 'Edit bang' })).toHaveTextContent('bangSFX')
    expect(screen.getByRole('button', { name: 'Edit locked' })).toHaveTextContent(
      'lockedSmall Text',
    )
    // Other layers keep Koharu's name for their kind.
    expect(screen.getByRole('button', { name: 'Edit caption' })).toHaveTextContent('captionText')
    expect(screen.getByRole('button', { name: 'Edit split' })).toHaveTextContent('splitBalloon')
  })
})
