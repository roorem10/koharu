import { vi } from 'vitest'

import { pageKey, pagesKey, projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { commands, type Change, type Layer, type Page } from '@koharu/ipc/protocol'

/** The values every layer has, as a new one gets them. */
const fresh = { visible: true, locked: false, opacity: 1, blend: 'normal', attributes: {} } as const

/** A layer as the scene sends it: `fields`, its kind's among them, over a new layer's values. */
function layer(id: string, fields: object): Layer {
  return { id, name: id, ...fresh, ...fields } as Layer
}

/** A block reading `source` in a box, with its own mask and clean-up, as detection leaves one. */
export function block(id: string, source: string, fields: object = {}): Layer {
  return layer(id, {
    kind: 'block',
    source: {
      content: [{ insert: source }],
      frame: { box: { bounds: { x: 0, y: 0, width: 40, height: 80 } } },
    },
    mask: { id: `${id}-mask`, name: '', ...fresh },
    cleanup: { id: `${id}-cleanup`, name: '', ...fresh },
    ...fields,
  })
}

function balloon(id: string, children: Layer[], fields: object = {}): Layer {
  return layer(id, { kind: 'balloon', outline: 'M0 0H100V100H0Z', children, ...fields })
}

/** A page with each case the box actions tell apart: a balloon whose text is split over two
 * blocks, beside a sound effect; a balloon holding one block; a locked balloon; a block outside
 * any balloon; a locked block; and a text layer. */
export function boxesPage(): Page {
  return {
    id: 'page',
    size: { width: 800, height: 1200 },
    background: null,
    layers: [
      layer('panel', {
        kind: 'panel',
        outline: 'M0 0H800V600H0Z',
        children: [
          balloon('split', [
            block('first', 'Hel'),
            block('second', 'lo'),
            block('bang', 'Bang', { role: 'sfx' }),
          ]),
          balloon('single', [block('alone', 'Hi')]),
          balloon('held', [block('held-first', 'Wa'), block('held-second', 'it')], {
            locked: true,
          }),
        ],
      }),
      block('free', 'Free'),
      block('locked', 'Locked', { locked: true }),
      layer('caption', { kind: 'text', content: [{ insert: 'Caption' }] }),
    ],
  }
}

/** The page's texts in reading order, as the scene's story lists them. */
export const story = [
  'first',
  'second',
  'bang',
  'alone',
  'held-first',
  'held-second',
  'free',
  'locked',
  'caption',
]

export const change = {
  revision: 'r1',
  created: [],
  pages: { added: [], changed: ['page'], removed: [], reordered: false },
  layers: [],
  history: { steps: [], current: 0 },
} as Change

/** Every command answered without the desktop, each edit answering `change`, Redo boxes finding no
 * box fixed by hand or locked, Add subtitle answering the subtitle "bang-subtitle", and Reset to
 * lettering default resetting one text. */
export function desktop() {
  const table = commands as unknown as Record<string, (...args: unknown[]) => Promise<unknown>>
  for (const name of Object.keys(table)) vi.spyOn(table, name).mockResolvedValue(null)
  return {
    run: vi.spyOn(commands, 'runPipeline').mockResolvedValue(1),
    countRedo: vi
      .spyOn(commands, 'booharuCountRedo')
      .mockImplementation(async (blocks) => ({ ready: blocks.length, hand_fixed: 0, locked: 0 })),
    redo: vi
      .spyOn(commands, 'booharuRedoBoxes')
      .mockResolvedValue({ run: 1, hand_fixed: 0, locked: 0 }),
    join: vi.spyOn(commands, 'booharuJoinBalloon').mockResolvedValue(change),
    joinPage: vi.spyOn(commands, 'booharuJoinPage').mockResolvedValue(change),
    split: vi.spyOn(commands, 'booharuSplitBlock').mockResolvedValue({
      change,
      halves: ['first', 'first-half'],
      run: 2,
    }),
    note: vi.spyOn(commands, 'booharuSetReviewNote').mockResolvedValue(change),
    subtitle: vi
      .spyOn(commands, 'booharuAddSfxSubtitle')
      .mockResolvedValue({ layer: 'bang-subtitle', run: null }),
    reset: vi
      .spyOn(commands, 'booharuResetLettering')
      .mockResolvedValue({ changed: 1, locked: 0, untranslated: 0 }),
    lettering: vi.spyOn(commands, 'booharuGetLettering').mockResolvedValue([]),
    findSplit: vi
      .spyOn(commands, 'booharuFindSplitBalloons')
      .mockResolvedValue([{ page: 'page', balloon: 'split', blocks: ['first', 'second'] }]),
    show: vi.spyOn(commands, 'show').mockResolvedValue(null),
  }
}

/** The project open with `page`, the canvas showing `shown`, and Story's read of `page` and the
 * canvas's of the shown one ready. */
export function openProject(page: Page, shown: string | null = page.id): void {
  const project: ProjectView = {
    name: 'Demo',
    page: shown,
    pages: [{ id: page.id, size: page.size, background: null }],
    history: { steps: [], current: 0 },
    revisions: {},
  }
  queryClient.setQueryData(projectKey, project)
  queryClient.setQueryData(pageKey, shown === page.id ? page : null)
  queryClient.setQueryData([...pagesKey, page.id], { page, story })
}
