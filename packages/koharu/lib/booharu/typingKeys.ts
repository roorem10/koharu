'use client'

import { useEffect } from 'react'

import { controlKeepsKey } from '@/lib/booharu/keys'
import { pressTyped } from '@/lib/booharu/words'
import { useKoharuStore } from '@/lib/store'

/** Which of B and I a key press asks for: Ctrl, or Cmd, with B or I, without Alt or Shift, while
 * no input method composes. The letter is the one the layout types, as the canvas reads its own
 * chords; on a layout whose keys type no Latin letter, such as Russian, it's the key's place, B's
 * or I's on a US keyboard. */
export function typingShortcut(event: KeyboardEvent): 'bold' | 'italic' | null {
  if (!(event.ctrlKey || event.metaKey) || event.altKey || event.shiftKey) return null
  if (event.isComposing || event.keyCode === 229) return null
  const key = event.key.toLowerCase()
  const letter = /^[a-z]$/.test(key)
    ? key
    : ({ KeyB: 'b', KeyI: 'i' } as Record<string, string>)[event.code]
  return letter === 'b' ? 'bold' : letter === 'i' ? 'italic' : null
}

/** Ctrl+B and Ctrl+I (Cmd on a Mac) while the canvas types into a text: B and I for its selected
 * characters, or at a caret for the next ones typed, by the rules the type bar's B and I and the
 * Story panel's word toolbar share, off taking the mark off for real (S11-d). A key a focused
 * control keeps, as a text field keeps every key, the Story panel's included, is left to it, as is
 * one a handler already took. Mounted once, among the providers' hooks. */
export function useTypingShortcuts(): void {
  useEffect(() => {
    const down = (event: KeyboardEvent) => {
      const shortcut = typingShortcut(event)
      if (!shortcut || event.defaultPrevented || controlKeepsKey(event)) return
      const typing = useKoharuStore.getState().typing
      if (!typing) return
      event.preventDefault()
      pressTyped(shortcut, typing)
    }
    window.addEventListener('keydown', down)
    return () => window.removeEventListener('keydown', down)
  }, [])
}
