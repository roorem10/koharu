import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '0.83.5') }))

describe('AboutDialog Booharu identity', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_VERSION', undefined)
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_COMMIT', undefined)
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllEnvs()
  })

  it('shows the Booharu version and author, and credits Koharu in one line', async () => {
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_VERSION', 'Booharu 0.0.4')
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_COMMIT', '1a2b3c4d')
    const { AboutDialog } = await import('@/components/app/AboutDialog')

    render(<AboutDialog open onOpenChange={vi.fn()} />)

    expect(await screen.findByText('Based on Koharu 0.83.5 by Mayo Takanashi.')).toBeInTheDocument()
    expect(screen.getByText('Roorem10')).toBeInTheDocument()
    expect(screen.getByText('0.0.4')).toHaveAttribute('title', '1a2b3c4d')
    expect(screen.getByRole('dialog')).not.toHaveTextContent('1a2b3c4d')
    expect(screen.queryByText('Fork version')).not.toBeInTheDocument()
  })

  it('shows a work-order build name unchanged', async () => {
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_VERSION', 'WO-01c')
    const { AboutDialog } = await import('@/components/app/AboutDialog')

    render(<AboutDialog open onOpenChange={vi.fn()} />)

    expect(await screen.findByText('WO-01c')).toBeInTheDocument()
  })
})
