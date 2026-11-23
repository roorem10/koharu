import { QueryClientProvider } from '@tanstack/react-query'
import { act, render } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { Reader } from '@/components/editor/Reader'
import { projectKey, queryClient, type ProjectView } from '@/lib/queries'
import { commands } from '@koharu/ipc/protocol'

import { desktop, openChapter, settle } from './reader'

function reader() {
  return (
    <QueryClientProvider client={queryClient}>
      <Reader />
    </QueryClientProvider>
  )
}

describe("Koharu's reader ends Booharu's typing", () => {
  it('ends the canvas typing once as it mounts, though it shows no page then', async () => {
    desktop()
    // The first page, alone in its spread with nothing facing it: the reader sends no show, whose
    // commit of the typing would end it too.
    openChapter(3, 'p1')
    const view = render(reader())
    await settle()
    expect(commands.booharuFinishTyping).toHaveBeenCalledOnce()
    expect(commands.show).not.toHaveBeenCalled()

    // Drawn again, as a change to the project draws it, it ends nothing more.
    act(() => {
      const project = queryClient.getQueryData<ProjectView>(projectKey)!
      queryClient.setQueryData(projectKey, { ...project, revisions: { p1: 'r2' } })
    })
    view.rerender(reader())
    await settle()
    expect(commands.booharuFinishTyping).toHaveBeenCalledOnce()
  })
})
