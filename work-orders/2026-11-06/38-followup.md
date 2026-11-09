# WO-38 follow-up: keep box notes in the sidecar

This worktree's last commit (agent and MCP coverage) moved box notes out of the project sidecar into a new Scene component (`BoxNote`, `BOX_NOTE = 19` in `crates/koharu-scene/src/schema.rs`, `crates/koharu-scene/src/components/notes.rs`). That's a save-format change, and notes in projects saved by 0.0.3 aren't migrated, so users would lose them. It was not allowed by the work order, and a sibling branch reads notes from the sidecar's `LineContext.box_note`.

Undo this one part only:

1. Remove the Scene `BoxNote` component, its schema entry, exports and validation. Put `crates/koharu-scene` back exactly as it was before this branch's commit (`git show HEAD~1:<path>` shows the earlier version; don't use git commands that change the working tree or history, edit the files).
2. Put box notes back in the sidecar, as on `HEAD~1`, including `crates/booharu-context/src/sidecar.rs`, `crates/koharu-app/src/commands/notes.rs`, `packages/koharu/components/editor/BoxNoteLine.tsx`, `packages/koharu/tests/components/box-notes.test.tsx` and the box note types in `protocol.ts`.
3. Keep the agent tools `get_box_note` and `set_box_note`, implemented through the same native function the UI's note command uses, on the sidecar. Keep read-only filtering correct.
4. Leave everything else from this branch untouched.

Run the box-notes frontend test. Summarize what changed.
