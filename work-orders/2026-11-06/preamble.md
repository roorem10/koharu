# Booharu work order

You are working on Booharu, a personal Windows fork of Koharu (manga scanlation editor: Tauri + CEF desktop app, Rust backend in `crates/`, Next.js/React frontend in `packages/koharu`, generated bindings in `packages/bridge/src/protocol.ts`). The current directory is a git worktree already on this work order's branch, based on the `booharu` branch.

## Rules for every work order

- Read `AGENTS.md` first and follow it (typed commands, Scene/patch ownership, one undo step per committed change, no backward-compatibility layers, no drive-by cleanup, smallest coherent change).
- Do NOT run `git commit`, `git push`, `git checkout`, `git switch`, `git stash` or `git reset`. Leave your work as uncommitted changes in the working tree. A reviewer commits it.
- `cargo` and `rustc` are NOT installed on this PC, so Rust cannot be compiled or tested here. Write Rust as if there is no second chance: open and read every type, trait, function signature and import you use, follow the patterns of neighbouring code, check borrow and async lock usage by reading, and add `#[cfg(test)]` tests for real regression boundaries anyway. Never leave pseudo-code, `todo!()`, or placeholder calls.
- Frontend checks DO work. From `packages/koharu`, run a focused test with `node ../../node_modules/vitest/vitest.mjs run <test-file>`. Type-check with `node ../../node_modules/typescript/lib/tsc.js --noEmit -p .` if useful (compare against errors that already exist before your change, such as the missing canvas wasm). Do not run the full test suite; run only the tests for what you touched. There is no `bun` and no `next dev` here.
- Bindings: `packages/bridge/src/protocol.ts` is generated from specta and can't be regenerated here. When you add or change a Tauri command or an exported type, update `protocol.ts` by hand in exactly the shape the generator would emit (copy neighbouring entries), register the command wherever the app's command list lives, and say in your final summary that bindings need regenerating when a Rust toolchain is available.
- User-visible strings go through the translation function. Add every new key to `packages/koharu/public/locales/en-US/translation.json` AND to every other locale folder, translated properly into that language. Don't lock translated phrases in tests.
- Standing parity rule (issue #51): every new user-facing feature adds or extends an agent action in `KoharuHost::tools()` (`crates/koharu-app/src/commands/agent/host.rs`), which also reaches the MCP server. Tools that edit stay hidden in MCP read-only mode; check how read-only filtering works and keep it correct. Agent edits stay revisioned and undoable, like user edits.
- Auto-run goal rules (issue #54), where they apply: automatic first, but hand edits always win and running again never overwrites one. Every step also works on its own through a button or menu entry. Set it once in Settings. Every automatic decision shows its reason (for example "Shout: spiky bubble") and is one undo step. No save-format change unless there's no other way; prefer the project sidecar, app settings and existing `extensions` maps.
- Other work orders are being built in parallel on sibling branches from the same base and will be combined later. To keep merges clean, put new code in new files or modules where that fits the code, and keep edits to shared files (command registration, `host.rs`, big components, locale files) small and additive. Do not rename or reorganize anything.
- Don't use computer use, browser or desktop-control tools: this is the owner's own PC and they may be using it. Check UI through component tests only. Screenshots aren't required for this work.
- Don't add progress notes or new markdown docs to the repo. Don't touch version numbers, workflows, release files, `CHANGELOG.md` or `FORK_VERSION`.
- Finish the work completely rather than stopping to ask. If something is truly ambiguous, choose the option that best fits the existing code and record it in the summary.
- If the job is too large to finish well, finish a coherent, working subset and state clearly what's left.

## When you finish

End with a plain summary: what changed (by file or area), what you verified and how, what could not be verified without Rust, any decision you made on your own, and short manual test steps for the Windows build.

---

