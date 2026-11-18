'use client'

import { ScanText } from 'lucide-react'
import { useTranslation } from 'react-i18next'

/** The O tool's entry in Koharu's tool rail: Koharu's ToolBar names it by its id, `tools.ocr_box`,
 * shows its key from `toolKeys`, and selects it on a click, as it does each of its own tools. */
export const ocrBoxTool = ['ocr_box', ScanText] as const

/** The O tool's other modes, on a line of their own under its name and key in its tooltip: Alt
 * reads only, Shift reads, translates and cleans. app/booharu.css lets the tooltip wrap for it. */
export function OcrBoxHint() {
  const { t } = useTranslation()
  return (
    <span className='booharu-tool-hint text-[10px] opacity-80'>
      {t('booharu.tools.ocrBoxModifiers')}
    </span>
  )
}
