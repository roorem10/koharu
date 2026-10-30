export const discussLineEvent = 'booharu-discuss-line'

export function discussLine(prompt: string) {
  window.dispatchEvent(new CustomEvent(discussLineEvent, { detail: prompt }))
}

export const openContextEvent = 'booharu-open-context'

export const historyChangedEvent = 'booharu-history-changed'
