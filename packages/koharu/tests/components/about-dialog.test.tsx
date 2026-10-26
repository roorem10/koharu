import { cleanup, render, screen } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('@tauri-apps/api/app', () => ({ getVersion: vi.fn(async () => '0.83.5') }))

describe('AboutDialog fork build identity', () => {
  beforeEach(() => {
    vi.resetModules()
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_VERSION', undefined)
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_COMMIT', undefined)
  })

  afterEach(() => {
    cleanup()
    vi.unstubAllEnvs()
  })

  it('shows the work-order version and build commit beside the upstream identity', async () => {
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_VERSION', 'WO-01c')
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_COMMIT', '1a2b3c4d')
    const { AboutDialog } = await import('@/components/app/AboutDialog')

    render(<AboutDialog open onOpenChange={vi.fn()} />)

    expect(await screen.findByText('0.83.5')).toBeInTheDocument()
    expect(screen.getByText('Mayo Takanashi')).toBeInTheDocument()
    expect(screen.getByText('Fork version')).toBeInTheDocument()
    expect(screen.getByText('WO-01c · 1a2b3c4d')).toBeInTheDocument()
  })

  it('shows only the work-order version when no build commit is available', async () => {
    vi.stubEnv('NEXT_PUBLIC_KOHARU_FORK_VERSION', 'WO-01c')
    const { AboutDialog } = await import('@/components/app/AboutDialog')

    render(<AboutDialog open onOpenChange={vi.fn()} />)

    expect(await screen.findByText('0.83.5')).toBeInTheDocument()
    expect(screen.getByText('Fork version')).toBeInTheDocument()
    expect(screen.getByText('WO-01c')).toBeInTheDocument()
    expect(screen.getByRole('dialog')).not.toHaveTextContent('·')
  })

  it('hides the fork row when the build has no fork metadata', async () => {
    const { AboutDialog } = await import('@/components/app/AboutDialog')

    render(<AboutDialog open onOpenChange={vi.fn()} />)

    expect(await screen.findByText('0.83.5')).toBeInTheDocument()
    expect(screen.getByText('Mayo Takanashi')).toBeInTheDocument()
    expect(screen.queryByText('Fork version')).not.toBeInTheDocument()
    expect(screen.getByRole('dialog').querySelectorAll('dt')).toHaveLength(2)
  })
})
