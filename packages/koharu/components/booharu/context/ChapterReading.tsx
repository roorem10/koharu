'use client'

import { ChapterBriefLine } from '@/components/booharu/context/ChapterBriefLine'
import { ReadChapterButton } from '@/components/booharu/context/ReadChapterButton'
import type { FullChapter } from '@/lib/booharu/context'

/** Read chapter and the brief it writes, in the Context tab between the series and the glossary:
 * the Read chapter button, then the brief line with View. */
export function ChapterReading({ chapter }: { chapter: FullChapter }) {
  return (
    <section className='grid min-w-0 gap-2'>
      <ReadChapterButton linked={chapter.series !== null} />
      <ChapterBriefLine brief={chapter.context.brief} />
    </section>
  )
}
