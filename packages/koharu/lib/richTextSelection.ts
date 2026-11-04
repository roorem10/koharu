import { create } from 'zustand'

import type { RichTextSelection } from '@/lib/richText'

/**
 * Words selected in the on-page editor. WO-11D sets and clears it (it must be
 * cleared when edit mode ends or another box is selected); WO-11E reads it to
 * make the panel act on those words. Shared by the foundation; don't add fields.
 */
export const useRichTextSelection = create<{
  selection: RichTextSelection | null
  setSelection: (selection: RichTextSelection | null) => void
}>()((set) => ({
  selection: null,
  setSelection: (selection) => set({ selection }),
}))
