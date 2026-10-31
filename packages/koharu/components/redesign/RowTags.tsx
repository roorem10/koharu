'use client'

import { useTranslation } from 'react-i18next'

import type { EntityId } from '@koharu/bridge/protocol'

import { useRedesignStore } from './state'

import styles from './row-tags.module.css'

export function RowTags({
  layer,
  name,
  hasEffects,
  issues,
}: {
  layer: EntityId
  name: string
  hasEffects: boolean
  issues: number
}) {
  const { t } = useTranslation()
  const openFx = useRedesignStore((state) => state.openFx)
  const openDetails = useRedesignStore((state) => state.openDetails)
  const button =
    'grid h-6 min-w-6 shrink-0 place-items-center rounded-md px-1 text-[11px] hover:bg-foreground/10 focus-visible:ring-2 focus-visible:ring-ring'
  return (
    <div
      className='flex shrink-0 items-center gap-0.5'
      onPointerDown={(event) => event.stopPropagation()}
      onMouseDown={(event) => event.stopPropagation()}
    >
      {hasEffects && (
        <button
          type='button'
          className={`${button} font-semibold italic`}
          aria-label={t('redesign.effectsFor', { name })}
          onClick={(event) => {
            event.stopPropagation()
            openFx([layer])
          }}
        >
          fx
        </button>
      )}
      <button
        type='button'
        className={`${button} ${issues > 0 ? styles.issue : 'text-muted-foreground'}`}
        data-state={issues > 0 ? 'blink' : 'idle'}
        aria-label={
          issues > 0
            ? t('redesign.detailsIssues', { name, count: issues })
            : t('redesign.detailsFor', { name })
        }
        onClick={(event) => {
          event.stopPropagation()
          openDetails(layer)
        }}
      >
        ?
      </button>
    </div>
  )
}
