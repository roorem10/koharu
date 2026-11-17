import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '0.85.0') }))

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
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_VERSION', 'Booharu 0.0.5')
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_COMMIT', '1a2b3c4d')
    const { AboutDialog } = await import('@/components/app/AboutDialog')

    render(<AboutDialog open onOpenChange={vi.fn()} />)

    expect(await screen.findByText('Based on Koharu 0.85.0 by Mayo Takanashi.')).toBeInTheDocument()
    expect(screen.getByText('roorem10')).toBeInTheDocument()
    expect(screen.getByText('0.0.5')).toHaveAttribute('title', '1a2b3c4d')
    expect(screen.getByRole('dialog')).not.toHaveTextContent('1a2b3c4d')
  })

  it('says the version is unavailable when the build has none', async () => {
    const { AboutDialog } = await import('@/components/app/AboutDialog')

    render(<AboutDialog open onOpenChange={vi.fn()} />)

    expect(await screen.findByText('Unavailable')).toBeInTheDocument()
  })
})
