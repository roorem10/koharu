# Koharu handoff — Preview 2 complete; temporary hold

The first two additions and the requested follow-up corrections are complete. The user postponed the Preview 1 trial and authorized isolated application data plus the outline fixes. The project is back on hold after delivering Preview 2. Do not begin Perspective Transform, rich text or Mesh Warp without a new user instruction.

## Current delivery

Use Koharu-Typesetting-Preview-2-Portable.zip (already saved as libfile_431dea2ed6f08191aba256d2c73e1640) or Koharu-Typesetting-Preview-2-Setup.exe. Preserve all Preview 1 files as historical baselines. Preview 1 shared official data paths and is not the isolated trial build. The Preview 2 full source includes the cumulative patch against the original source and a Preview-1-to-2 patch for Claude or another developer; each patch is checked against its actual baseline.

Read IMPLEMENTATION_HANDOFF.md for the complete implementation, PREVIEW2_IMPLEMENTATION.md for the follow-up, FEATURE_REQUESTS.md for all five original requirements, FEATURE_PROGRESS.md for checks/manual acceptance, and PREVIEW_INSTALL.md for beginner trial instructions. Read AGENTS.md before future changes.

## Implementation summary

koharu-config::preview selects executable-relative PreviewData without automatic legacy reads, migration or path fallback. Projects, settings, model/runtime packages, browser storage, logs and window state use that root. Native secrets use a separate OS credential service. Startup rejects the official identifier; the updater plugin is omitted. HF model downloads use the existing pinned HTTPS resolver and managed downloader to avoid Xet's user-global cache. Sentry remains disabled in the private build. No global Windows environment variables are changed.

Text contours are flattened at 0.02 layout pixels and nonzero-unioned with i_overlay 9.1.0. Opaque Outside strokes paint behind the fill; clear/translucent fills keep isolated outside clipping. Manual and detected new text explicitly starts in Contour. Legacy values retain Expansion. Editable text, glyph resources, shared rendering/export, typography revision 2 and prepared manifest version 5 remain intact.

## Verification

Two profile tests, two contour geometry/pixel regressions and a prepared-packet round trip passed. Rust formatting/diff checks passed. Windows x64 release compiled. Exact installer directory and all 242 runtime hashes, 242 portable entries, all 10 complete x64 PEs, and 237 CEF archive members passed. The CEF archive SHA-1 matches the official index. Two local extraction outputs were incomplete; full archive member streams matched expected sizes/hashes. A temporary staging copy was excluded and the inflated installer candidate discarded. Only the clean verified setup is delivered.

No Windows GUI launch, real favorites restart test, problem-font trial, save/reopen, Undo/Redo or desktop canvas/export comparison has been claimed. These remain user acceptance checks. Preview project edits save automatically; new/updated revision-2 typography cannot be opened by official 0.83.5. Use new or copied projects.

## Persistence and recovery

Ongoing source checkpoint: libfile_d518d0b2b028819196e928266d4513c3. Ongoing checklist: libfile_eb0443fa33e881919dc7b23a6f7c2b71. Implementation handoff: libfile_7c4e7aac02008191bfa3d3fbc7e04e68. Install guide: libfile_182b954fec408191a685498e513ce953. Resolve exact Preview 2 filenames for the installer/frozen source; never overwrite the original upload. Use retained receipts or current metadata for version guards.

Save compiled downloads as well as source, so a future session can recover them without rebuilding SDKs and CEF. Source archives exclude credentials, model weights, PreviewData, build caches and internal review screenshots. Do not run Preview 1 build helpers unchanged: preserve frozen names. The Windows script now targets Preview-2 outputs and refuses to wipe staging that contains PreviewData. A future usage-limit interruption is not authorization to resume the three held additions.

## Historical Preview 1 handoff

The first-two-features delivery is complete: favorite fonts and contour outlines. **The user explicitly requested a temporary hold. Do not begin Perspective Transform, rich text or Mesh Warp unless the user resumes the project.** Those additions remain unimplemented.

Read IMPLEMENTATION_HANDOFF.md for exact file ownership, algorithms, reproduction/build steps, tests and limitations; FEATURE_PROGRESS.md for the checklist and pending Windows acceptance; PREVIEW_INSTALL.md for trial instructions; BUILD_PREVIEW.md and scripts/build-preview-windows.ps1 for native Windows rebuilding. Read AGENTS.md before future changes.

## Frozen delivery

- Koharu-Typesetting-Preview-1-Setup.exe: warning-free Windows x64 setup, validated against all 242 staged runtime files.
- Koharu-Typesetting-Preview-1-Portable.zip: same application/runtime; every entry matches staging by size and SHA256.
- Koharu-Typesetting-Preview-1-Source.zip: full source plus Koharu-Typesetting-Preview-1.patch beside the koharu-main folder.
- Koharu-Implementation-Handoff.md, Koharu-Preview-Install.md and Koharu-Progress.md: standalone documents.

The original koharu-main.zip is unchanged. The exact patch was checked/applied on a fresh extraction and all changed text files match the modified source. Original archive SHA256 and reproduction steps are in the implementation handoff. Generated WASM is included in the full source for recovery, is rebuilt from Rust, and is excluded from the text patch.

## Durable identities and recovery

Ongoing source checkpoint: libfile_d518d0b2b028819196e928266d4513c3. Ongoing checklist: libfile_eb0443fa33e881919dc7b23a6f7c2b71. Portable: libfile_c591fc0bfd7881918305d5184f920709. Installer: libfile_8871491ae620819193f1efedabfcc965. Implementation handoff: libfile_7c4e7aac02008191bfa3d3fbc7e04e68. Install guide: libfile_182b954fec408191a685498e513ce953. Resolve the exact frozen source filename if its local bytes are missing. Use current metadata/retained receipts for version guards; never guess a version or overwrite the original upload.

Temporary tools were cleared during an earlier usage-limit interruption; the checkpoint preserved both implementations and generated bindings/WASM. Compiled downloads are now saved too. Recover them instead of rebuilding merely to inspect the project. Local Git history was recreated from recovered source, so old local commit objects may be unavailable; the frozen source/patch are authoritative.

Two DLLs and some local installer-extraction outputs were incomplete. Complete members from the intact archives matched expected sizes/hashes; the final portable and installer contents passed verification. Use the current saved portable, not its superseded initial checkpoint. The extraction problem did not require a dependency-version or compiled-application-code change.

## Verification and build constraints

Focused rendering/packet, picker/localization, TypeScript, lint, formatting and component screenshot checks passed. Native bindings were genuinely regenerated. Windows x64 release and private frontend rebuilt successfully. All 10 runtime binaries parse as x64; no separate Visual C++ runtime imports were found. All 237 CEF runtime/notice files match the verified archive. Actual Windows GUI launch, restart persistence, project save/reopen, Undo/Redo, the user's problem font and final canvas/export comparison remain user acceptance checks.

Keep private-preview isolation for any future preview rebuild: NEXT_PUBLIC_KOHARU_PREVIEW=1, KOHARU_PRIVATE_PREVIEW=1, NEXT_TELEMETRY_DISABLED=1, and remove SENTRY_* / NEXT_PUBLIC_SENTRY_* variables. Automatic approval review rejected an earlier unsanitized pipeline for potential build-data disclosure; do not resume it. The safe config skips Sentry's plugin before import and browser/native reporting is disabled. Normal upstream behavior remains available outside preview mode.

Direct Cargo releases require --features tauri/custom-protocol. Keep the Windows Rust +crt-static flag together with preview build.windows.staticVCRuntime=false, avoiding Tauri's conflicting UCRT override. Exact tool versions, targeted clang-cl/environment fixes and packaging verification are documented in the implementation handoff. No dependency versions were silently downgraded; no GitHub push, PR or release was made.

If the user resumes, materialize the frozen source/latest checkpoint, read their latest instructions and trial feedback, and address requested milestone issues before later additions. Preserve new checkpoints and the frozen Preview 1 baseline. Do not assume an old temporary workspace, SDK cache or review screenshots survived. There is no active implementation task while the project is on hold.
