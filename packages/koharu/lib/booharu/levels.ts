'use client'

// Page levels (S14-k to S14-o): Fix page levels stretches a page's scan from its darkest ink to
// its lightest paper and can whiten its paper, keeping the scan as it came in a hidden, locked
// Original scan layer, which Restore original scan puts back. Each runs by AGENTS.md's command
// rules: the page menu's status and the dialog's preview are quiet reads, as one only adds the
// Restore item and the other shows its own failure; Fix is a mutation the Activity center lists,
// whose answer the dialog shows; Stop needs no answer; Restore toasts what it did.

import { useMutation, useQuery } from '@tanstack/react-query'
import type { ParseKeys } from 'i18next'
import { useEffect, useState } from 'react'
import { create } from 'zustand'

import { call } from '@/lib/booharu/call'
import { saveBooharuSection } from '@/lib/booharu/preferences'
import i18n from '@/lib/i18n'
import { projectKey, queryClient, send, useProject, type ProjectView } from '@/lib/queries'
import {
  commands,
  events,
  type LevelsProgress,
  type LevelsReport,
  type LevelsSettings,
  type LevelsSkip,
  type PageId,
} from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** `[booharu-levels]` with every value set. */
export type Levels = Required<LevelsSettings>

/** The section while the file sets nothing, as booharu-core's defaults are. */
export const builtInLevels: Levels = {
  auto_levels: true,
  clip_percent: 0.5,
  whiten: false,
  threshold: 235,
}

/** The largest share of a page's pixels, in percent, left out at each end: just under half, so
 * some are always left between them, as booharu-core's `MAX_CLIP_PERCENT`. */
export const MAX_CLIP_PERCENT = 49.9

/** `section` with every value: one the desktop leaves out is the built-in one, as booharu-core
 * reads a missing key. */
export function completeLevels(section: LevelsSettings | undefined): Levels {
  return {
    auto_levels: section?.auto_levels ?? builtInLevels.auto_levels,
    clip_percent: section?.clip_percent ?? builtInLevels.clip_percent,
    whiten: section?.whiten ?? builtInLevels.whiten,
    threshold: section?.threshold ?? builtInLevels.threshold,
  }
}

/** Whether `clip` is a share to leave out at each end: a number from 0 to 49.9 percent. */
export function isClipPercent(clip: number | null): clip is number {
  return clip !== null && Number.isFinite(clip) && clip >= 0 && clip <= MAX_CLIP_PERCENT
}

/** Whether `threshold` is a whitening threshold: a whole number from 0 to 255. */
export function isThreshold(threshold: number | null): threshold is number {
  return threshold !== null && Number.isInteger(threshold) && threshold >= 0 && threshold <= 255
}

/** Whether a fix as `levels` say would change anything: it levels, whitens, or both. */
export function fixes(levels: Levels): boolean {
  return levels.auto_levels || levels.whiten
}

/** Saves `change` over `[booharu-levels]` as the file holds it just before the save, with every
 * other section as the file holds it then, and answers the section as saved. A failure shows as
 * every failed command's does. */
export async function saveLevels(change: Partial<Levels>): Promise<Levels> {
  const saved = await saveBooharuSection('booharu-levels', (section) => ({
    ...completeLevels(section),
    ...change,
  }))
  return completeLevels(saved['booharu-levels'])
}

/** Which pages a fix takes: those the dialog opened on (`selected`), or every page. */
export type LevelsScope = 'selected' | 'all'

interface LevelsState {
  /** The pages Selected pages fixes: those the page menu acted on, or the pages selected in the
   * page list as the Edit menu opened the dialog; null while the dialog is closed. */
  selected: PageId[] | null
  /** The scope the dialog opened with: Selected pages from the page menu, All pages from the Edit
   * menu. */
  scope: LevelsScope
  /** The project the dialog opened in, which it closes without. */
  project: string | null
  /** How far the fix going is, as the desktop told it last; none before its first page. */
  progress: LevelsProgress | null
}

/** The levels dialog's state, the interface's own, apart from the project and Koharu's store. */
export const useLevels = create<LevelsState>()(() => ({
  selected: null,
  scope: 'all',
  project: null,
  progress: null,
}))

/** Opens the levels dialog on `selected`, the pages Selected pages fixes, in the project's order,
 * set to `scope`. */
export function openLevels(selected: readonly PageId[], scope: LevelsScope): void {
  const project = queryClient.getQueryData<ProjectView | null>(projectKey)?.name ?? null
  useLevels.setState({ selected: inProjectOrder(selected), scope, project, progress: null })
}

/** Closes the levels dialog. */
export function closeLevels(): void {
  useLevels.setState({ selected: null, progress: null })
}

/** `pages` in the open project's order, leaving out any it no longer holds. */
export function inProjectOrder(pages: readonly PageId[]): PageId[] {
  const project = queryClient.getQueryData<ProjectView | null>(projectKey)
  return (project?.pages ?? []).map((page) => page.id).filter((page) => pages.includes(page))
}

/** The pages a page's menu acts on: the pages selected in the page list when `page`, the page the
 * menu is for, is among them, otherwise `page` alone. */
export function menuPages(page: PageId, selected: readonly PageId[]): PageId[] {
  return selected.includes(page) ? inProjectOrder(selected) : [page]
}

/** The key of every page's levels status, under the project's, so the change, undo or redo of any
 * page, which reads the project again, reads it again too. */
export const levelsStatusKey = [...projectKey, 'booharu', 'levels-status'] as const

/** How each of `pages` stands for a fix or a restore: whether it has an Original scan, and why a
 * fix would skip it, read for each project anew, and again after every change. A cheap read that
 * decodes nothing. A failed read shows no toast: it only adds the Restore item. */
export function useLevelsStatus(pages: PageId[], enabled = true) {
  const project = useProject().data?.name ?? null
  return useQuery({
    queryKey: [...levelsStatusKey, project ?? '', ...pages],
    queryFn: () => commands.booharuGetLevelsStatus(pages),
    enabled: enabled && project !== null && pages.length > 0,
    meta: { booharuQuiet: true },
  })
}

/** What a fix as `levels` say would do to `page`: its black and white points and the scan before
 * and after, read for each project anew and dropped as soon as nothing shows it, as each read
 * levels the whole page. A failed read shows no toast: the dialog says it couldn't. */
export function useLevelsPreview(page: PageId | null, levels: Levels) {
  const project = useProject().data?.name ?? null
  return useQuery({
    queryKey: ['booharu', 'levels-preview', project ?? '', page ?? '', levels],
    queryFn: () => commands.booharuPreviewLevels(page ?? '', levels),
    enabled: project !== null && page !== null && fixes(levels),
    gcTime: 0,
    meta: { booharuQuiet: true },
  })
}

/** The Activity center's row while a fix goes. */
const fixing: ParseKeys = 'booharu.levels.fixing'

/** The key of the dialog's fix, while it goes. */
export const fixLevelsKey = ['booharu', 'fix-page-levels'] as const

/** Whether the dialog's Stop was pressed since the fix going started. */
let stopPressed = false

/** Fix page levels: levels `pages` as `levels` say, one undo step, answering what it changed and
 * skipped, or null when the dialog's Stop ended it, which then changed no page, as the dialog says.
 * The Activity center lists it while it goes. Any other failure, a refusal or a stop from
 * elsewhere, such as Koharu's Stop or the project closing, shows as every failed command's does. */
export function useFixLevels() {
  return useMutation({
    mutationKey: fixLevelsKey,
    mutationFn: async ({
      pages,
      levels,
    }: {
      pages: PageId[]
      levels: Levels
    }): Promise<LevelsReport | null> => {
      stopPressed = false
      useLevels.setState({ progress: null })
      try {
        return await commands.booharuFixPageLevels(pages, levels)
      } catch (error) {
        if (stopPressed) return null
        throw error
      }
    },
    meta: { activity: fixing },
  })
}

/** The dialog's Stop: stops the fix going, which then changes no page; nothing else Booharu runs
 * stops. */
export function stopLevels(): void {
  stopPressed = true
  send(commands.booharuStopLevels)
}

/** Follows the fix going, page by page, while mounted: the dialog's progress line. */
export function useLevelsProgress(): void {
  useEffect(() => {
    let active = true
    const listening = events.booharuLevelsProgress((progress) => {
      if (active) useLevels.setState({ progress })
    })
    void listening.catch(() => undefined)
    return () => {
      active = false
      void listening.then((stop) => stop()).catch(() => undefined)
    }
  }, [])
}

/** The string key that says why a fix or a restore left a page as it was. */
export function skipReason(reason: LevelsSkip): ParseKeys {
  return `booharu.levels.reason.${reason}`
}

/** `page` as the page list names it, by its place in the open project: Page 3; its id when the
 * project no longer holds it. */
export function pageName(page: PageId): string {
  const pages = queryClient.getQueryData<ProjectView | null>(projectKey)?.pages ?? []
  const index = pages.findIndex((candidate) => candidate.id === page)
  return index < 0 ? page : i18n.t('navigator.pageNumber', { number: index + 1 })
}

/** Each page `report` skipped, as a line: the page's name and why. */
export function skippedLines(report: LevelsReport): string[] {
  return report.skipped.map(({ page, reason }) =>
    i18n.t('booharu.levels.skipDetail', {
      page: pageName(page),
      reason: i18n.t(skipReason(reason)),
    }),
  )
}

/** Restore original scan: puts back the Original scan of `pages` as their scan, one undo step, with
 * a toast of how many it restored and why it skipped any. A failure shows as every failed
 * command's does, and rejects. */
export async function restoreOriginals(pages: PageId[]): Promise<LevelsReport> {
  const report = await call(commands.booharuRestoreOriginalPages, pages)
  const skipped = skippedLines(report)
  toast.add({
    type: 'info',
    title: i18n.t('booharu.levels.restored', { count: report.changed.length }),
    description:
      skipped.length > 0
        ? `${i18n.t('booharu.levels.skipped', { count: skipped.length })} ${skipped.join(' · ')}`
        : undefined,
  })
  return report
}

/** A base64 PNG, as the preview gives it, as a Blob URL while the component using it is mounted
 * and the image is the same: each one replaced, or left on unmounting, is revoked. */
export function usePngUrl(base64: string | undefined): string | null {
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    if (base64 === undefined) {
      setUrl(null)
      return
    }
    const bytes = Uint8Array.from(atob(base64), (char) => char.charCodeAt(0))
    const next = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))
    setUrl(next)
    return () => URL.revokeObjectURL(next)
  }, [base64])
  return url
}
