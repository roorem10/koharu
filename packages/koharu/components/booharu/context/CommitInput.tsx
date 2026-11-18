'use client'

import { useCallback, useEffect, useRef, useState, type ComponentProps } from 'react'

import { Input } from '@koharu/ui/components/input'

type CommitInputProps = Omit<
  ComponentProps<typeof Input>,
  'value' | 'defaultValue' | 'onChange'
> & {
  value: string
  /** Whether a blank field goes back to `value` instead of being committed, as a name it needs. */
  required?: boolean
  /** Commits `value`; a promise that rejects puts back what's saved. */
  onCommit: (value: string) => Promise<unknown> | void
}

/** A one-line field, as Koharu's CommitTextarea is a text box, that commits what's written once:
 * as it loses the focus, on Enter, or as it goes, such as when its dialog closes on Escape. Escape
 * first takes back an edit. Enter confirming an IME composition commits nothing. While it isn't
 * being edited, it follows `value`, as another writer changes it. */
export function CommitInput({ value, required, onCommit, ...props }: CommitInputProps) {
  const [draft, setDraft] = useState(value)
  const editing = useRef(false)
  // The text last committed, until `value` comes back, so a blur after Enter commits nothing more.
  const sent = useRef<string | null>(null)
  const latest = useRef({ value, required, onCommit, draft })

  useEffect(() => {
    latest.current = { value, required, onCommit, draft }
  })

  useEffect(() => {
    sent.current = null
    if (!editing.current) setDraft(value)
  }, [value])

  const commit = useCallback((text: string) => {
    const { value: saved, required: needed, onCommit: save } = latest.current
    if (text === (sent.current ?? saved)) return
    if (needed && !text.trim()) {
      setDraft(saved)
      return
    }
    sent.current = text
    void Promise.resolve(save(text)).catch(() => {
      sent.current = null
      if (!editing.current) setDraft(latest.current.value)
    })
  }, [])

  // What's written as it goes is committed too.
  useEffect(() => () => commit(latest.current.draft), [commit])

  return (
    <Input
      {...props}
      value={draft}
      onFocus={(event) => {
        editing.current = true
        props.onFocus?.(event)
      }}
      onChange={(event) => setDraft(event.currentTarget.value)}
      onBlur={(event) => {
        editing.current = false
        commit(draft)
        props.onBlur?.(event)
      }}
      onKeyDown={(event) => {
        if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
          event.preventDefault()
          commit(draft)
        } else if (event.key === 'Escape' && draft !== (sent.current ?? value)) {
          // Escape takes back the edit; another goes on to close the dialog it's in.
          event.stopPropagation()
          setDraft(sent.current ?? value)
        }
        props.onKeyDown?.(event)
      }}
    />
  )
}
