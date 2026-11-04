import { useRichTextSelection } from '@/lib/richTextSelection'

// WO-11D owns this file.
export function mockWo11d() {
  useRichTextSelection.setState({ selection: null })
}
