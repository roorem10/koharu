'use client'

import type { TFunction } from 'i18next'
import { useEffect, useMemo } from 'react'
import { create } from 'zustand'

import { call } from '@/lib/booharu/call'
import { useKoharuStore, type Panel } from '@/lib/store'
import {
  commands,
  events,
  type BooharuQc,
  type Finding,
  type LayerId,
  type PageId,
  type QcSettings,
} from '@koharu/ipc/protocol'

/** The quality check's findings in the open project, as the desktop told them, kept apart from
 * Koharu's store with the right pane's tab before QC, which the pane goes back to as QC empties. */
interface QcState {
  /** The project the findings are of; none while none is open. */
  project: string | null
  /** Each page's findings, hidden ones included, in reading order and then the checks' order, as
   * the desktop told them last; a page with none has no entry. */
  pages: Record<PageId, Finding[]>
  /** The right pane's last tab other than QC. */
  previous: Exclude<Panel, 'qc'> | null
}

export const useBooharuQc = create<QcState>()(() => ({ project: null, pages: {}, previous: null }))

/** Box details' section that lists what the quality check found on a text. */
export const qcSection = 'qc'

/** The pages told since the last read of every finding was asked, whose findings are newer than its
 * answer. */
const told = new Set<PageId>()

/** Counts the reads of every finding, so only the last one asked for lands. */
let reads = 0

/** `qc`, a page's findings as the desktop checked them last, in place of the page's earlier ones. */
export function receiveQc({ page, findings }: BooharuQc): void {
  told.add(page)
  useBooharuQc.setState((state) => {
    const pages = { ...state.pages }
    if (findings.length) pages[page] = findings
    else delete pages[page]
    return { pages }
  })
}

/** Reads every finding the desktop holds for `project`, in place of the store's, apart from the
 * pages told since the read was asked. A read that fails leaves the findings as they are: the next
 * check of each page tells it again. */
async function seed(project: string): Promise<void> {
  const read = ++reads
  told.clear()
  const findings = await commands.booharuGetQc().catch(() => null)
  const state = useBooharuQc.getState()
  if (!Array.isArray(findings) || read !== reads || state.project !== project) return
  const pages: Record<PageId, Finding[]> = {}
  for (const finding of findings) (pages[finding.page] ??= []).push(finding)
  for (const page of told) {
    const newer = state.pages[page]
    if (newer) pages[page] = newer
    else delete pages[page]
  }
  useBooharuQc.setState({ pages })
}

/** Holds `project`'s findings: none until they're read as another project opens, or the open one
 * closes; read again when the open one is told again, as the desktop tells it after changes the
 * interface missed. */
function hold(project: string | null): void {
  if (useBooharuQc.getState().project !== project) useBooharuQc.setState({ project, pages: {} })
  if (project !== null) void seed(project)
}

/** How many of `pages`' findings are open: not marked OK while their box stays as it is. */
export function openCount(pages: Record<PageId, Finding[]>): number {
  let count = 0
  for (const findings of Object.values(pages)) {
    for (const finding of findings) if (!finding.hidden) count += 1
  }
  return count
}

/** Shows the tab open before QC once QC has nothing open to list, as its tab hides then (T2). */
function leaveEmptyQc(): void {
  const { panel, showPanel } = useKoharuStore.getState()
  if (panel !== 'qc') return
  const { pages, previous } = useBooharuQc.getState()
  if (openCount(pages) === 0) showPanel(previous ?? 'layers')
}

/** Keeps the quality check's findings with the open project: read from the desktop as a project
 * opens, or as the interface loads with one open, then replaced page by page as each page is
 * checked again; and remembers the right pane's last tab other than QC, which the pane goes back to
 * once QC has nothing open to list. It never shows the QC tab by itself. Listening starts first, so
 * nothing told while the findings load is missed. Mounted once, in the app's providers. */
export function useBooharuQcEvents(): void {
  useEffect(() => {
    let active = true
    let toldProject = false
    const listening = [
      events.booharuQc((qc) => {
        if (active) receiveQc(qc)
      }),
      events.project((name) => {
        if (!active) return
        toldProject = true
        hold(name)
      }),
    ].map((listener) => listener.catch(() => null))
    void Promise.all(listening)
      .then(() => commands.getOpenProject())
      .then((name) => {
        // A project told since is newer than this answer.
        if (active && !toldProject) hold(name)
      })
      .catch(() => undefined)

    const remember = (panel: Panel) => {
      if (panel !== 'qc') useBooharuQc.setState({ previous: panel })
    }
    remember(useKoharuStore.getState().panel)
    const panels = useKoharuStore.subscribe((state, before) => {
      if (state.panel === before.panel) return
      remember(state.panel)
      leaveEmptyQc()
    })
    const findings = useBooharuQc.subscribe((state, before) => {
      if (state.pages !== before.pages) leaveEmptyQc()
    })
    return () => {
      active = false
      panels()
      findings()
      for (const listener of listening) void listener.then((stop) => stop?.())
    }
  }, [])
}

/** How many findings in the open project are open, each a row of the QC tab, which shows while
 * there is one. */
export function useQcOpenCount(): number {
  return useBooharuQc((state) => openCount(state.pages))
}

const none: Finding[] = []

/** The findings on `layer` of `page`, hidden ones included, in the order the desktop told them. */
export function useLayerFindings(page: PageId | null | undefined, layer: LayerId): Finding[] {
  const findings = useBooharuQc((state) => (page ? state.pages[page] : undefined) ?? none)
  return useMemo(() => findings.filter((finding) => finding.layer === layer), [findings, layer])
}

/** How many findings on `layer` of `page` are open: the count its ? tag tells. */
export function useLayerQcCount(page: PageId | null | undefined, layer: LayerId): number {
  return useBooharuQc((state) => {
    let count = 0
    for (const finding of (page ? state.pages[page] : undefined) ?? none) {
      if (finding.layer === layer && !finding.hidden) count += 1
    }
    return count
  })
}

/** Which findings the QC tab lists: every one, the automatic checks', or the agent's flags. */
export type QcFilter = 'all' | 'auto' | 'agent'

export const qcFilters = ['all', 'auto', 'agent'] as const satisfies readonly QcFilter[]

/** Whether the QC tab lists `finding` under `filter`. */
export function matchesFilter(finding: Finding, filter: QcFilter): boolean {
  return filter === 'all' || finding.source === filter
}

/** A page's rows in the QC tab. */
export interface QcGroup {
  page: PageId
  /** The page's place in the project's order now, from 1. */
  number: number
  findings: Finding[]
}

/** The QC tab's rows: each page's open findings `filter` lists, and its hidden ones too with
 * `showHidden`, in the order the desktop told them, under the page's number in the project's page
 * order, `order`; a page with nothing listed, or one the project no longer lists, is left out. */
export function qcGroups(
  pages: Record<PageId, Finding[]>,
  order: readonly PageId[],
  filter: QcFilter,
  showHidden: boolean,
): QcGroup[] {
  return order.flatMap((page, index) => {
    const findings = (pages[page] ?? []).filter(
      (finding) => (showHidden || !finding.hidden) && matchesFilter(finding, filter),
    )
    return findings.length ? [{ page, number: index + 1, findings }] : []
  })
}

/** What `finding` found, in the user's language, as the QC tab and Box details word it: the agent's
 * own message for its flag. */
export function qcLabel(t: TFunction, { problem }: Finding): string {
  switch (problem.kind) {
    case 'overflow':
      return t('booharu.qc.checks.overflow')
    case 'too_small':
      return t('booharu.qc.checks.too_small', { size: problem.size })
    case 'small_word':
      return t('booharu.qc.checks.small_word', { size: problem.size, word: problem.word })
    case 'missing_translation':
      return t('booharu.qc.checks.missing_translation')
    case 'empty_source':
      return t('booharu.qc.checks.empty_source')
    case 'leftover_source':
      return t(
        problem.unchanged ? 'booharu.qc.checks.unchanged' : 'booharu.qc.checks.leftover_source',
      )
    case 'odd_font':
      return t('booharu.qc.checks.odd_font', {
        expected: problem.expected ?? t('booharu.qc.defaultFont'),
      })
    case 'glossary':
      return t('booharu.qc.checks.glossary', { source: problem.source, target: problem.target })
    case 'flag':
      return problem.message
  }
}

/** Marks `finding` OK while its box stays as it is, or with `ok` false takes its check's marks on
 * the box back: one undo step, after which the desktop checks the page again at once and tells the
 * store. A refusal, as for a box locked since, shows as every failed command's does. */
export function markQcOk(finding: Finding, ok: boolean): Promise<unknown> {
  return call(commands.booharuMarkQcOk, finding.layer, finding.check, ok)
}

/** Dismisses the agent's flag `finding` is, that message alone: one undo step, after which the
 * desktop checks the page again at once and tells the store. Any other finding dismisses nothing. */
export function dismissFlag(finding: Finding): Promise<unknown> {
  if (finding.problem.kind !== 'flag') return Promise.resolve(null)
  return call(commands.booharuDismissFlag, finding.layer, finding.problem.message)
}

/** `[booharu-qc]`'s smallest readable text while the file sets none, as booharu-core's default:
 * 1% of the page's shorter side. */
export const DEFAULT_MIN_SIZE = 0.01

/** The percentages of the page's shorter side Settings takes for the smallest readable text. */
export const MIN_SIZE_PERCENT = { min: 0.1, max: 10, step: 0.1 } as const

/** `settings`' smallest readable text as Settings shows it: a percentage of the page's shorter side,
 * to two decimals. */
export function minSizePercent(settings: QcSettings | undefined): number {
  return Math.round((settings?.min_size ?? DEFAULT_MIN_SIZE) * 10_000) / 100
}

/** The share of the page's shorter side `percent` saves as `min_size`; null for a percentage
 * Settings doesn't take: none, or one outside 0.1 to 10. */
export function minSizeShare(percent: number | null): number | null {
  if (percent === null || !Number.isFinite(percent)) return null
  if (percent < MIN_SIZE_PERCENT.min || percent > MIN_SIZE_PERCENT.max) return null
  return Math.round(percent * 100) / 10_000
}
