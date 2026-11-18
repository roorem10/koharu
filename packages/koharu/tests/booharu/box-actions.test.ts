import { beforeEach, describe, expect, it } from 'vitest'

import { boxActions, selectedBoxes, type BoxSelection } from '@/lib/booharu/boxActions'
import { useBoxDetails } from '@/lib/booharu/boxDetails'
import { useKoharuStore } from '@/lib/store'

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
    expect(enabled(['first'])).toEqual({ ...runs, join: true, split: true, details: true })
    // Its part stands for it.
    expect(enabled(['second-mask'])).toEqual(enabled(['second']))
    // Several blocks: the runs only.
    expect(enabled(['first', 'free'])).toEqual({ ...none, ...runs })
    // A text layer: Box details only, and a text layer beside a block leaves the runs to it.
    expect(enabled(['caption'])).toEqual({ ...none, details: true })
    expect(enabled(['caption', 'free'])).toEqual({ ...none, ...runs })
    // A locked block, or one in a locked balloon, or a locked one among others: no change at all.
    expect(enabled(['locked'])).toEqual({ ...none, details: true })
    expect(enabled(['held-first'])).toEqual({ ...none, details: true })
    expect(enabled(['free', 'locked'])).toEqual(none)
    // A balloon with one text block, or a block in none, has nothing to join with.
    expect(enabled(['alone'])).toEqual({ ...runs, join: false, split: true, details: true })
    expect(enabled(['free'])).toEqual({ ...runs, join: false, split: true, details: true })
    // A sound effect is neither joined nor split.
    expect(enabled(['bang'])).toEqual({ ...runs, join: false, split: false, details: true })
    expect(enabled([])).toEqual(none)
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
    const { run: pipeline, join, split } = desktop()

    await run('read', ['first', 'caption', 'free-cleanup'])
    await run('translate', ['second'])
    await run('clean', ['alone'])
    expect(pipeline.mock.calls).toEqual([
      [{ blocks: ['first', 'free'] }, ['recognize']],
      [{ blocks: ['second'] }, ['translate']],
      [{ blocks: ['alone'] }, ['inpaint']],
    ])

    await run('split', ['first-mask'])
    expect(split).toHaveBeenCalledWith('first')

    // The text that went with the join leaves the selection to the one it joined into.
    useKoharuStore.getState().selectLayers(['second', 'free'])
    await run('join', ['second'])
    expect(join).toHaveBeenCalledWith('second')
    expect(useKoharuStore.getState().selectedLayers).toEqual(['first', 'free'])

    await run('details', ['caption'])
    expect(useBoxDetails.getState().shown).toEqual({ page: 'page', layer: 'caption' })
  })
})
