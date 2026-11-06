import { QueryClientProvider } from '@tanstack/react-query'
import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { StartView } from '@/components/start/StartView'
import { queryClient, useProject } from '@/lib/queries'
import { commands } from '@koharu/bridge/protocol'

function ProjectFlow() {
  const project = useProject().data
  return project ? <p>Opened {project.name}</p> : <StartView />
}

function renderProjectFlow() {
  return render(
    <QueryClientProvider client={queryClient}>
      <ProjectFlow />
    </QueryClientProvider>,
  )
}

describe('StartView', () => {
  afterEach(() => vi.restoreAllMocks())

  it('creates a managed project by name', async () => {
    let opened = false
    vi.spyOn(commands, 'getProject').mockImplementation(async () =>
      opened
        ? {
            name: 'Volume 1',
            revision: 0,
            active_page: null,
            can_undo: false,
            can_redo: false,
          }
        : null,
    )
    vi.spyOn(commands, 'listProjects').mockResolvedValue([])
    const create = vi.spyOn(commands, 'createProject').mockImplementation(async () => {
      opened = true
      return null
    })
    renderProjectFlow()
    expect(await screen.findByRole('heading', { name: 'Projects' })).toBeInTheDocument()
    fireEvent.change(screen.getByRole('textbox', { name: 'Project name' }), {
      target: { value: 'Volume 1' },
    })
    const createButton = screen.getByRole('button', { name: 'Create' })
    await waitFor(() => expect(createButton).toBeEnabled())
    fireEvent.click(createButton)
    await waitFor(() => expect(create).toHaveBeenCalledWith('Volume 1', null))
    expect(await screen.findByText('Opened Volume 1')).toBeInTheDocument()
  })

  it('opens a managed project without reloading the application', async () => {
    let opened = false
    vi.spyOn(commands, 'getProject').mockImplementation(async () =>
      opened
        ? {
            name: 'Blue Archive',
            revision: 1,
            active_page: null,
            can_undo: false,
            can_redo: false,
          }
        : null,
    )
    vi.spyOn(commands, 'listProjects').mockResolvedValue([{ name: 'Blue Archive' }])
    const open = vi.spyOn(commands, 'openProject').mockImplementation(async () => {
      opened = true
      return null
    })
    renderProjectFlow()

    const project = (await screen.findByText('Blue Archive')).closest('button')
    if (!project) throw new Error('project row is not interactive')
    await waitFor(() => expect(project).toBeEnabled())
    fireEvent.click(project)

    await waitFor(() => expect(open).toHaveBeenCalledWith('Blue Archive'))
    expect(await screen.findByText('Opened Blue Archive')).toBeInTheDocument()
  })

  it('includes the offered series when creating a new chapter', async () => {
    vi.spyOn(commands, 'listProjects').mockResolvedValue([])
    vi.spyOn(commands, 'listSeries').mockResolvedValue([{ id: 'series', name: 'A series' }])
    vi.spyOn(commands, 'getLastSeries').mockResolvedValue({ id: 'series', name: 'A series' })
    const create = vi.spyOn(commands, 'createProject').mockResolvedValue(null)
    render(
      <QueryClientProvider client={queryClient}>
        <StartView />
      </QueryClientProvider>,
    )
    fireEvent.change(screen.getByRole('textbox', { name: 'Project name' }), {
      target: { value: 'Chapter 2' },
    })
    const button = screen.getByRole('button', { name: 'Create' })
    await waitFor(() => expect(button).toBeEnabled())
    fireEvent.click(button)
    await waitFor(() => expect(create).toHaveBeenCalledWith('Chapter 2', 'series'))
  })

  it('confirms before deleting a managed project', async () => {
    vi.spyOn(commands, 'listProjects')
      .mockResolvedValueOnce([{ name: 'Blue Archive' }])
      .mockResolvedValueOnce([])
    const remove = vi.spyOn(commands, 'deleteProject').mockResolvedValue(null)
    render(
      <QueryClientProvider client={queryClient}>
        <StartView />
      </QueryClientProvider>,
    )

    const deleteButton = await screen.findByRole('button', { name: 'Delete Blue Archive' })
    await waitFor(() => expect(deleteButton).toBeEnabled())
    fireEvent.click(deleteButton)

    expect(screen.getByRole('alertdialog')).toHaveTextContent(
      'This permanently deletes “Blue Archive” and all of its pages.',
    )
    expect(remove).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Delete project' }))
    await waitFor(() => expect(remove).toHaveBeenCalledWith('Blue Archive'))
  })
})
