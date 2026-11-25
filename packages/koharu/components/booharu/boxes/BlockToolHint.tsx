'use client'

import { useTranslation } from 'react-i18next'

/** The modes of Koharu's text block tool, K, whose box Booharu reads at once, on a line of their
 * own under its name and key in its tooltip: Alt reads only, Shift reads, translates and cleans, as
 * the O tool's did before it folded into K. app/booharu.css lets the tooltip wrap for it. */
export function BlockToolHint() {
  const { t } = useTranslation()
  return (
    <span className='booharu-tool-hint text-[10px] opacity-80'>
      {t('booharu.tools.ocrBoxModifiers')}
    </span>
  )
}
