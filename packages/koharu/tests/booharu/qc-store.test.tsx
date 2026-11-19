import { renderHook } from '@testing-library/react'
import { act } from 'react'
import { beforeEach, describe, expect, it } from 'vitest'

import {
  receiveQc,
  useBooharuQc,
  useBooharuQcEvents,
  useLayerFindings,
  useLayerQcCount,
  useQcOpenCount,
} from '@/lib/booharu/qc'
import { useKoharuStore } from '@/lib/store'
import type { Finding } from '@koharu/ipc/protocol'

import { finding, forgetFindings, listenToEvents, qcDesktop, settle, tell } from './qc'

const overflow = finding('p1', 'a')
const small = finding('p1', 'a', { kind: 'too_small', size: 9.5, limit: 12 })
const flag = finding('p2', 'b', { kind: 'flag', message: 'Check the name' })

/** The app's listener, as the providers mount it. */
function listen() {
  return renderHook(() => useBooharuQcEvents())
}

/** A deferred answer to every finding's read, given with `answer`. */
function later(read: ReturnType<typeof qcDesktop>['read']) {
  let answer: (findings: Finding[]) => void = () => undefined
  read.mockImplementationOnce(() => new Promise((resolve) => (answer = resolve)))
  return (findings: Finding[]) => act(() => answer(findings))
}

beforeEach(() => {
  listenToEvents()
  forgetFindings()
})

describe('Booharu quality check store', () => {
  it('reads every finding as the interface loads with a project open, and again as each opens', async () => {
    const { desk, read } = qcDesktop('Demo', { Demo: [overflow, small, flag] })
    listen()
    await settle()
    expect(useBooharuQc.getState()).toMatchObject({
      project: 'Demo',
      pages: { p1: [overflow, small], p2: [flag] },
    })

    // The desktop closes the project, then opens another.
    tell('project', null)
    expect(useBooharuQc.getState().project).toBeNull()
    expect(useBooharuQc.getState().pages).toEqual({})
    desk.open = 'Other'
    desk.findings.Other = [finding('p9', 'z')]
    tell('project', 'Other')
    expect(useBooharuQc.getState().pages).toEqual({})
    await settle()
    expect(useBooharuQc.getState().pages).toEqual({ p9: [finding('p9', 'z')] })
    expect(read).toHaveBeenCalledTimes(2)
  })

  it('replaces a page’s findings as the desktop checks it again, dropping a page left with none', async () => {
    qcDesktop('Demo', { Demo: [overflow, small, flag] })
    listen()
    await settle()

    const fixed = finding('p1', 'a', { kind: 'missing_translation' })
    tell('booharuQc', { page: 'p1', findings: [fixed] })
    expect(useBooharuQc.getState().pages).toEqual({ p1: [fixed], p2: [flag] })
    tell('booharuQc', { page: 'p2', findings: [] })
    expect(useBooharuQc.getState().pages).toEqual({ p1: [fixed] })
  })

  it('keeps the pages told while the findings load over the read’s older answer', async () => {
    const { read } = qcDesktop('Demo')
    const answer = later(read)
    listen()
    await settle()

    const newer = finding('p1', 'a', { kind: 'empty_source' })
    tell('booharuQc', { page: 'p1', findings: [newer] })
    tell('booharuQc', { page: 'p2', findings: [] })
    answer([overflow, flag, finding('p3', 'c')])
    await settle()
    expect(useBooharuQc.getState().pages).toEqual({ p1: [newer], p3: [finding('p3', 'c')] })
  })

  it('keeps the findings while the open project is told again, and lands only its latest read', async () => {
    const { desk, read } = qcDesktop('Demo', { Demo: [overflow] })
    listen()
    await settle()

    // Told again, as after changes the interface missed: what shows stays until the read lands.
    desk.findings.Demo = [overflow, flag]
    tell('project', 'Demo')
    expect(useBooharuQc.getState().pages).toEqual({ p1: [overflow] })
    await settle()
    expect(useBooharuQc.getState().pages).toEqual({ p1: [overflow], p2: [flag] })

    // Told twice, the first read answering last: its answer is the older one.
    const first = later(read)
    tell('project', 'Demo')
    desk.findings.Demo = [small]
    tell('project', 'Demo')
    await settle()
    expect(useBooharuQc.getState().pages).toEqual({ p1: [small] })
    first([overflow, flag])
    await settle()
    expect(useBooharuQc.getState().pages).toEqual({ p1: [small] })

    // The project closes before the read for it answers.
    const closed = later(read)
    tell('project', 'Demo')
    desk.open = null
    tell('project', null)
    closed([small])
    await settle()
    expect(useBooharuQc.getState().project).toBeNull()
    expect(useBooharuQc.getState().pages).toEqual({})
  })

  it('counts the open findings, those marked OK left out, in the project and on one layer', () => {
    const hidden = { ...small, hidden: true }
    act(() => {
      receiveQc({ page: 'p1', findings: [overflow, hidden, finding('p1', 'other')] })
      receiveQc({ page: 'p2', findings: [finding('p2', 'a'), flag] })
    })

    const { result } = renderHook(() => ({
      open: useQcOpenCount(),
      layer: useLayerQcCount('p1', 'a'),
      findings: useLayerFindings('p1', 'a'),
      none: useLayerFindings(null, 'a'),
    }))
    expect(result.current).toEqual({
      open: 4,
      layer: 1,
      findings: [overflow, hidden],
      none: [],
    })

    act(() => receiveQc({ page: 'p1', findings: [{ ...overflow, hidden: true }, hidden] }))
    expect(result.current.open).toBe(2)
    expect(result.current.layer).toBe(0)
  })

  it('goes back to the tab before QC once QC has nothing open, and never shows QC itself', async () => {
    qcDesktop('Demo')
    listen()
    await settle()
    const { showPanel } = useKoharuStore.getState()

    act(() => showPanel('story'))
    tell('booharuQc', { page: 'p1', findings: [overflow] })
    // Findings come in without leaving the tab shown.
    expect(useKoharuStore.getState().panel).toBe('story')

    act(() => showPanel('qc'))
    tell('booharuQc', { page: 'p1', findings: [{ ...overflow, hidden: true }] })
    expect(useKoharuStore.getState().panel).toBe('story')

    // The project closing empties QC as well.
    act(() => showPanel('context'))
    tell('booharuQc', { page: 'p2', findings: [flag] })
    act(() => showPanel('qc'))
    tell('project', null)
    expect(useKoharuStore.getState().panel).toBe('context')
    // QC shown with nothing open, its tab hidden, goes back at once.
    act(() => showPanel('qc'))
    expect(useKoharuStore.getState().panel).toBe('context')
  })
})
