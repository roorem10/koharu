import type * as Native from '@koharu/bridge/protocol'

// Bindings also describe sparse serde-default inputs. Commands serialize every field.
export type Serialized<T> = T extends (infer Item)[]
  ? Serialized<Item>[]
  : T extends object
    ? { [Key in keyof T]-?: Serialized<T[Key]> }
    : T

export type Series = Serialized<Native.Series>
export type GlossaryEntry = Serialized<Native.GlossaryEntry>
export type ChapterState = Serialized<Native.ChapterState>
export type ContextSettings = Serialized<Native.ContextSettings>
