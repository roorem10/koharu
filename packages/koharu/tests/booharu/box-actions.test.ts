import { beforeEach, describe, expect, it, vi } from 'vitest'

import { boxActions, selectedBoxes, type BoxSelection } from '@/lib/booharu/boxActions'
import { useBoxDetails } from '@/lib/booharu/boxDetails'
import { useKoharuStore } from '@/lib/store'
import type { BlockLettering, LetteringCategory, Page } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

import { block, boxesPage, desktop, openProject } from './boxes'

const page = boxesPage()

function selection(ids: string[], lettering?: BoxSelection['lettering']): BoxSelection {
  return { page: page.id, boxes: selectedBoxes(page.layers, ids), lettering }
}

/** Which actions are enabled for the texts `ids` name, by action. */
function enabled(ids: string[]): Record<string, boolean> {
  return Object.fromEntries(boxActions.map((action) => [action.id, action.enabled(selection(ids))]))
}

/** Runs the action `id` on the texts `ids` name, with the page's lettering when given. */
function run(id: string, ids: string[], lettering?: BoxSelection['lettering']): Promise<unknown> {
  const action = boxActions.find((candidate) => candidate.id === id)!
  return action.run(selection(ids, lettering))
}

/** The page's lettering as the desktop reads it, each block named in `categories` in its category,
 * by block. */
function pageLettering(
  categories: Record<string, LetteringCategory>,
): ReadonlyMap<string, BlockLettering> {
  return new Map(
    Object.entries(categories).map(([id, category]) => [
      id,
      { block: id, category, reason: 'user', label: null, shape: null, hand_edited: false },
    ]),
  )
}

const none = {
  fx: false,
  read: false,
  translate: false,
  clean: false,
  join: false,
  split: false,
  subtitle: false,
  category: false,
  reset: false,
  explain: false,
  details: false,
}
const runs = { read: true, translate: true, clean: true }
/** What any texts take: FX…, which opens Text effects over them, even locked, as the type bar
 * shows for them. */
const texts = { ...none, fx: true }

const translation = { content: [{ insert: 'Translated' }] }

/** The boxes page with the block outside any balloon, and the locked one, translated. */
function translatedPage(): Page {
  const translated = boxesPage()
  translated.layers[1] = block('free', 'Free', { translation })
  translated.layers[2] = block('locked', 'Locked', { locked: true, translation })
  return translated
}

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
    // A text block in a balloon with another: all of them but those a sound effect or a translation
    // asks for.
    expect(enabled(['first'])).toEqual({
      ...texts,
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
    expect(enabled(['first', 'free'])).toEqual({ ...texts, ...runs })
    // A text layer: Box details and Reset only, as it has no category, and a text layer beside a
    // block leaves the runs to it.
    expect(enabled(['caption'])).toEqual({ ...texts, reset: true, details: true })
    expect(enabled(['caption', 'free'])).toEqual({ ...texts, ...runs, reset: true })
    // A locked block, or one in a locked balloon: no change at all, though its line can be
    // explained, which changes nothing, and its text effects shown.
    expect(enabled(['locked'])).toEqual({ ...texts, explain: true, details: true })
    expect(enabled(['held-first'])).toEqual({ ...texts, explain: true, details: true })
    expect(enabled(['locked', 'held-first'])).toEqual(texts)
    // A locked one among others: the runs, which leave it alone and count it.
    expect(enabled(['free', 'locked'])).toEqual({ ...texts, ...runs })
    // A balloon with one text block, or a block in none, has nothing to join with.
    const alone = {
      ...texts,
      ...runs,
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

  it("offers Add subtitle for one unlocked sound effect, as its page's lettering says", () => {
    const subtitle = boxActions.find(({ id }) => id === 'subtitle')!
    const offered = (ids: string[], lettering?: BoxSelection['lettering']) =>
      subtitle.enabled(selection(ids, lettering))
    const lettering = pageLettering({ bang: 'sfx', first: 'normal_dialogue', free: 'sfx' })

    expect(offered(['bang'], lettering)).toBe(true)
    expect(offered(['bang-cleanup'], lettering)).toBe(true)
    // A block given the SFX category is a sound effect too, and one given another isn't, whatever
    // Koharu detected it as.
    expect(offered(['free'], lettering)).toBe(true)
    expect(offered(['bang'], pageLettering({ bang: 'shout' }))).toBe(false)
    expect(offered(['first'], lettering)).toBe(false)
    // Not while the lettering is unknown, nor for a block it doesn't list, a locked effect, or one
    // among other texts.
    expect(offered(['bang'])).toBe(false)
    expect(offered(['alone'], lettering)).toBe(false)
    expect(offered(['locked'], pageLettering({ locked: 'sfx' }))).toBe(false)
    expect(offered(['bang', 'free'], lettering)).toBe(false)
  })

  it('selects the subtitle it adds, or says a Translate run is making it', async () => {
    const { subtitle } = desktop()
    openProject(page)
    const shown = vi.spyOn(toast, 'add')
    const lettering = pageLettering({ bang: 'sfx' })

    // Made from the effect's translation, or the one it had, the subtitle is selected.
    await run('subtitle', ['bang-mask'], lettering)
    expect(subtitle).toHaveBeenCalledWith('bang', null)
    expect(useKoharuStore.getState().selectedLayers).toEqual(['bang-subtitle'])
    expect(shown).not.toHaveBeenCalled()

    // With no translation to make it from, the desktop translates the effect first.
    subtitle.mockResolvedValue({ layer: null, run: 4 })
    await run('subtitle', ['bang'], lettering)
    expect(shown).toHaveBeenCalledWith({
      type: 'info',
      title: 'Translating the sound effect for its subtitle…',
    })
  })

  it('resets every selected text to its lettering default and counts those it left', async () => {
    const { reset } = desktop()
    const shown = vi.spyOn(toast, 'add')
    const translated = translatedPage()
    const resetting = (ids: string[]): BoxSelection => ({
      page: translated.id,
      boxes: selectedBoxes(translated.layers, ids),
    })
    const action = boxActions.find(({ id }) => id === 'reset')!

    // A translated block or a text layer, neither locked: an untranslated block has no lettering.
    expect(action.enabled(resetting(['free']))).toBe(true)
    expect(action.enabled(resetting(['caption']))).toBe(true)
    expect(action.enabled(resetting(['first']))).toBe(false)
    expect(action.enabled(resetting(['locked']))).toBe(false)
    expect(action.enabled(resetting(['first', 'locked']))).toBe(false)
    // A block styled before it's translated: the desktop takes off the style its placeholder holds.
    const styled = boxesPage()
    const placeholder = { content: [], style: { chars: { size: 30 } }, frame: 'point' }
    styled.layers[1] = block('free', 'Free', { translation: placeholder })
    expect(action.enabled({ page: styled.id, boxes: selectedBoxes(styled.layers, ['free']) })).toBe(
      true,
    )

    // One such text is enough, and every selected one goes, the desktop counting those it leaves.
    reset.mockResolvedValue({ changed: 2, locked: 1, untranslated: 1 })
    await action.run(resetting(['free', 'first-mask', 'locked', 'caption']))
    expect(reset).toHaveBeenCalledWith(['free', 'first', 'locked', 'caption'])
    expect(shown).toHaveBeenLastCalledWith({
      type: 'success',
      title: 'Reset 2 boxes to their lettering default',
      description: '1 locked box was left as it is. 1 box has no translation to reset.',
    })

    reset.mockResolvedValue({ changed: 1, locked: 0, untranslated: 0 })
    await action.run(resetting(['free']))
    expect(shown).toHaveBeenLastCalledWith({
      type: 'success',
      title: 'Reset 1 box to its lettering default',
      description: undefined,
    })

    // None reset: the text that could be had its default already, and the locked one was left.
    reset.mockResolvedValue({ changed: 0, locked: 1, untranslated: 0 })
    await action.run(resetting(['free', 'locked']))
    expect(shown).toHaveBeenLastCalledWith({
      type: 'info',
      title: 'Nothing was reset',
      description: '1 locked box was left as it is.',
    })
    reset.mockResolvedValue({ changed: 0, locked: 0, untranslated: 0 })
    await action.run(resetting(['caption']))
    expect(shown).toHaveBeenLastCalledWith({
      type: 'info',
      title: 'Already at the lettering default',
      description: undefined,
    })
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
