import { describe, expect, it, vi } from 'vitest'

import { queryClient } from '@/lib/queries'
import { toast } from '@koharu/ui/components/toast'

describe("Booharu's quiet reads", () => {
  it('shows no toast for a failed read carrying booharuQuiet', async () => {
    const shown = vi.spyOn(toast, 'add')

    await expect(
      queryClient.fetchQuery({
        queryKey: ['booharu', 'quiet-read'],
        queryFn: () => Promise.reject('The series file is not valid.'),
        meta: { booharuQuiet: true },
      }),
    ).rejects.toBe('The series file is not valid.')

    expect(shown).not.toHaveBeenCalled()
  })

  it("shows any other failed read as Koharu's error toast, once", async () => {
    const shown = vi.spyOn(toast, 'add')

    await expect(
      queryClient.fetchQuery({
        queryKey: ['booharu', 'read'],
        queryFn: () => Promise.reject('The series folder is gone.'),
      }),
    ).rejects.toBe('The series folder is gone.')

    expect(shown).toHaveBeenCalledTimes(1)
    expect(shown).toHaveBeenCalledWith(
      expect.objectContaining({ type: 'error', description: 'The series folder is gone.' }),
    )
  })
})
