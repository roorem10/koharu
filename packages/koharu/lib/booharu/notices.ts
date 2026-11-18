'use client'

import { useEffect } from 'react'

import i18n from '@/lib/i18n'
import { events, type BooharuNotice } from '@koharu/ipc/protocol'
import { toast } from '@koharu/ui/components/toast'

/** Shows each notice the desktop sends as `booharu_notice`, such as why a box drawn with the O
 * tool added nothing, as Koharu shows its canvas's notices: a toast in the interface's language. */
export function useBooharuNotices(): void {
  useEffect(() => {
    let active = true
    const listening = events.booharuNotice((notice) => {
      if (active) show(notice)
    })
    return () => {
      active = false
      void listening.then((stop) => stop()).catch(() => undefined)
    }
  }, [])
}

/** A refusal is told as Koharu tells its canvas's; a failure as an error, with what failed. */
function show(notice: BooharuNotice): void {
  if (notice.kind === 'failed') {
    toast.add({
      type: 'error',
      title: i18n.t('booharu.notice.failed', { message: notice.message }),
    })
  } else {
    toast.add({ type: 'info', title: i18n.t(`booharu.notice.${notice.kind}`) })
  }
}
