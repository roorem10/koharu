'use client'

import { X } from 'lucide-react'
import { useEffect, useId, useMemo, useRef, useState } from 'react'
import { useTranslation } from 'react-i18next'

import { boxDetailsSections } from '@/components/booharu/details/sections'
import { selectedBoxes } from '@/lib/booharu/boxActions'
import { closeBoxDetails, useBoxDetails } from '@/lib/booharu/boxDetails'
import { usePageStory, useProject } from '@/lib/queries'
import { Button } from '@koharu/ui/components/button'
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from '@koharu/ui/components/dialog'

/** Box details: one text's sections, from `boxDetailsSections`, opened from a Story row's ? tag and
 * menu and from the Edit menu. It follows the text through every change, reading its page as Story
 * does, and closes when the text goes, as a join can take it, and with the project. */
export function BoxDetailsDialog() {
  const { t } = useTranslation()
  const sections = useId()
  const shown = useBoxDetails((state) => state.shown)
  // What it showed last, kept while it closes.
  const [last, setLast] = useState(shown)
  if (shown && shown !== last) setLast(shown)
  const target = shown ?? last
  const project = useProject().data?.name
  const story = usePageStory(target?.page ?? '', shown !== null).data
  const box = useMemo(
    () => (target && story ? (selectedBoxes(story.page.layers, [target.layer])[0] ?? null) : null),
    [target, story],
  )

  useEffect(() => {
    if (shown && story && !box) closeBoxDetails()
  }, [shown, story, box])

  const opened = useRef(project)
  useEffect(() => {
    if (opened.current !== project) closeBoxDetails()
    opened.current = project
  }, [project])
  // The editor leaves when the project closes, and the text with it.
  useEffect(() => closeBoxDetails, [])

  return (
    <Dialog
      open={shown !== null && box !== null}
      onOpenChange={(open) => {
        if (!open) closeBoxDetails()
      }}
    >
      <DialogContent
        showCloseButton={false}
        className='flex max-h-[calc(100dvh-4rem)] flex-col gap-3 overflow-hidden p-4 sm:max-w-md'
      >
        <DialogTitle className='pr-8 text-[13px] font-semibold'>
          {t('booharu.details.title')}
        </DialogTitle>
        <DialogDescription className='sr-only'>
          {t('booharu.details.description')}
        </DialogDescription>
        <DialogClose
          render={
            <Button
              variant='ghost'
              size='icon-sm'
              className='absolute top-2 right-2'
              aria-label={t('booharu.details.close')}
            />
          }
        >
          <X />
        </DialogClose>
        {target && box && (
          <div className='-mx-4 grid min-h-0 gap-4 overflow-y-auto px-4 pb-1'>
            {boxDetailsSections.map(({ id, title, Section }) => (
              <section key={id} aria-labelledby={`${sections}-${id}`} className='grid gap-1.5'>
                <h3 id={`${sections}-${id}`} className='text-[11px] font-semibold'>
                  {t(title)}
                </h3>
                <Section key={box.layer.id} page={target.page} box={box} />
              </section>
            ))}
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
