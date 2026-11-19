import { beforeEach, describe, expect, it, vi } from 'vitest'

import { boxActions, selectedBoxes, type BoxSelection } from '@/lib/booharu/boxActions'
import { useBoxDetails } from '@/lib/booharu/boxDetails'
import { useKoharuStore } from '@/lib/store'
import { toast } from '@koharu/ui/components/toast'

import { boxesPage, desktop } from './boxes'

const page = boxesPage()

function selection(ids: string[]): BoxSelection {
  return { page: page.id, boxes: selectedBoxes(page.layers, ids) }
}

/** Which actions are enabled for the texts `ids` name, by action. */
function enabled(ids: string[]): Record<string, boolean> {
  return Object.fromEntries(boxActions.map((action) => [action.id, action.enabled(selection(ids))]))
}

/** Runs the action `id` on the texts `ids` name. */
function run(id: string, ids: string[]): Promise<unknown> {
  const action = boxActions.find((candidate) => candidate.id === id)!
  return action.run(selection(ids))
}

const none = {
  read: false,
  translate: false,
  clean: false,
  join: false,
  split: false,
  category: false,
  explain: false,
  details: false,
}
const runs = { read: true, translate: true, clean: true }

beforeEach(() => useBoxDetails.setState({ shown: null, reveal: null }))

describe('Booharu box actions', () => {
  it('takes a selected mask or clean-up for its block, and leaves other layers out', () => {
    const boxes = selectedBoxes(page.layers, ['first-mask', 'first', 'second-cleanup', 'split'])
    expect(boxes.map(({ layer, parent, locked }) => [layer.id, parent?.id, locked])).toEqual([
      ['first', 'split', false],
      ['second', 'split', false],
    ])
    expect(
      selectedBoxes(page.layers, ['caption', 'held-first']).map(({ locked }) => locked),
    ).toEqual([false, true])
  })

  it('enables each action only for a selection it can act on', () => {
    // A text block in a balloon with another: all of them.
    expect(enabled(['first'])).toEqual({
      ...runs,
      join: true,
      split: true,
      category: true,
      explain: true,
      details: true,
    })
    // Its part stands for it.
    expect(enabled(['second-mask'])).toEqual(enabled(['second']))
    // Several blocks: the runs only.
    expect(enabled(['first', 'free'])).toEqual({ ...none, ...runs })
    // A text layer: Box details only, as it has no category, and a text layer beside a block leaves
    // the runs to it.
    expect(enabled(['caption'])).toEqual({ ...none, details: true })
    expect(enabled(['caption', 'free'])).toEqual({ ...none, ...runs })
    // A locked block, or one in a locked balloon: no change at all, though its line can be
    // explained, which changes nothing.
    expect(enabled(['locked'])).toEqual({ ...none, explain: true, details: true })
    expect(enabled(['held-first'])).toEqual({ ...none, explain: true, details: true })
    expect(enabled(['locked', 'held-first'])).toEqual(none)
    // A locked one among others: the runs, which leave it alone and count it.
    expect(enabled(['free', 'locked'])).toEqual({ ...none, ...runs })
    // A balloon with one text block, or a block in none, has nothing to join with.
    const alone = {
      ...runs,
      join: false,
      split: true,
      category: true,
      explain: true,
      details: true,
    }
    expect(enabled(['alone'])).toEqual(alone)
    expect(enabled(['free'])).toEqual(alone)
    // A sound effect isn't joined with a balloon's text, but it is split, as the desktop splits it.
    expect(enabled(['bang'])).toEqual(alone)
    expect(enabled([])).toEqual(none)
  })

  it('leaves a hidden block out of a join, as the desktop does', () => {
    const hidden = boxesPage()
    hidden.layers[0]!.children![0]!.children![0]!.visible = false
    const join = boxActions.find(({ id }) => id === 'join')!
    // The hidden block takes no join, and the one text left showing has nothing to join with.
    for (const id of ['first', 'second']) {
      expect(join.enabled({ page: page.id, boxes: selectedBoxes(hidden.layers, [id]) })).toBe(false)
    }
  })

  it('refuses a join when any text of the balloon is locked', () => {
    const lockedText = boxesPage()
    const split = lockedText.layers[0]!.children![0]!
    split.children![1]!.locked = true
    const boxes = selectedBoxes(lockedText.layers, ['first'])
    const join = boxActions.find(({ id }) => id === 'join')!
    expect(join.enabled({ page: page.id, boxes })).toBe(false)
  })

  it('runs each one with its command, on the blocks alone', async () => {
    const { run: pipeline, countRedo, redo, join, split, lettering } = desktop()

    // Read and translate go through Redo boxes, which asks nothing while no box was fixed by hand;
    // clean runs at once.
    await run('read', ['first', 'caption', 'free-cleanup'])
    await run('translate', ['second'])
    await run('clean', ['alone'])
    expect(countRedo.mock.calls).toEqual([
      [['first', 'free'], ['recognize']],
      [['second'], ['translate']],
    ])
    expect(redo.mock.calls).toEqual([
      [['first', 'free'], ['recognize'], false],
      [['second'], ['translate'], false],
    ])
    expect(pipeline.mock.calls).toEqual([[{ blocks: ['alone'] }, ['inpaint']]])

    await run('split', ['first-mask'])
    expect(split).toHaveBeenCalledWith('first')

    // The text that went with the join leaves the selection to the one it joined into.
    useKoharuStore.getState().selectLayers(['second', 'free'])
    await run('join', ['second'])
    expect(join).toHaveBeenCalledWith('second')
    expect(useKoharuStore.getState().selectedLayers).toEqual(['first', 'free'])

    await run('details', ['caption'])
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'caption' })

    // Explain this line opens Box details on the line's explanation.
    await run('explain', ['second-cleanup'])
    expect(useBoxDetails.getState().shown).toEqual({
      page: 'page',
      layer: 'second',
      section: 'explain',
    })

    // Change category reads the page's lettering, then opens Box details on its section.
    await run('category', ['second-mask'])
    expect(lettering).toHaveBeenCalledWith('page')
    expect(useBoxDetails.getState().shown).toEqual({
      page: 'page',
      layer: 'second',
      section: 'lettering',
    })
  })

  it('reruns a selection holding a locked block, which the run leaves alone and counts', async () => {
    const { run: pipeline, countRedo, redo } = desktop()
    countRedo.mockResolvedValue({ ready: 1, hand_fixed: 0, locked: 1 })
    redo.mockResolvedValue({ run: 3, hand_fixed: 0, locked: 1 })
    const shown = vi.spyOn(toast, 'add')

    await run('translate', ['free', 'locked'])
    expect(redo).toHaveBeenCalledWith(['free', 'locked'], ['translate'], false)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'info', title: '1 locked box was left as it is.' }),
    )
    // Clean again asks nothing and runs on both, the stage leaving the locked one alone.
    await run('clean', ['free', 'locked'])
    expect(pipeline).toHaveBeenCalledWith({ blocks: ['free', 'locked'] }, ['inpaint'])
  })

  it('shows why Change category couldn’t read the lettering, and opens nothing', async () => {
    const { lettering } = desktop()
    const refused = "small text under 0 of the page's height isn't above 0 and at most 1"
    lettering.mockRejectedValue(refused)
    const shown = vi.spyOn(toast, 'add')

    await expect(run('category', ['second'])).rejects.toThrow(refused)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'error', description: refused }),
    )
    expect(useBoxDetails.getState().shown).toBeNull()
  })
})
