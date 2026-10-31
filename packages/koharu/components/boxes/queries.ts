'use client'

import { useKoharuStore } from '@/lib/store'

export function useBoxesBusy() {
  return useKoharuStore((state) => Object.values(state.jobs).some((job) => job.state === 'running'))
}
