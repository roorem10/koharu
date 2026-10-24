# Isolated Koharu Typesetting Preview 2 — implementation notes

Updated October 24, 2026. This is a focused follow-up to Preview 1. The frozen Preview 1 source and downloads are preserved. The user has not tested either build yet. Perspective Transform, rich text and Mesh Warp remain on hold.

## Requested changes

The user supplied Koharu-Review-Findings.md from Claude. Its feature review identified overlapping contour strokes, a possible antialiased seam in Outside outlines, and the legacy default for new text. The user also requested a test build that cannot automatically read or change official Koharu application data. Installing on another drive alone does not provide that separation.

## Data ownership

`crates/koharu-config/src/preview.rs` owns the private preview root. The existing private-build flag KOHARU_PRIVATE_PREVIEW=1 is compiled into the application; no user environment setup or launcher is required. The root is always a PreviewData directory beside the executable, resolved with current_exe, independent of the working directory. It has settings, projects, packages, browser and logs subdirectories. Startup creates and checks these directories and performs a checked write/remove probe. An inaccessible root stops startup. It never falls back to Documents/Koharu, ~/.koharu, an old installation's store or another user's cache. Existing settings/projects are not migrated.

`koharu-config::path()` chooses PreviewData/settings/config.toml. `ProjectLibrary::new()` chooses PreviewData/projects. `Store::root()` and desktop startup select PreviewData/packages for both downloaded model files and native inference packages. `koharu-secrets` selects the separate rs.koharu.typesetting-preview service in the operating-system credential store; credentials stay protected by Windows and do not travel in the portable folder. The new dependency from koharu-secrets to koharu-config has no cycle.

Desktop startup rejects the official app identifier when compiled as a private preview. It configures CEF's root_cache_path and the window's data_directory to PreviewData/browser, logs to PreviewData/logs, and window-state filename to an absolute path in PreviewData/settings. The window-state plugin can create an empty application-specific AppData directory as part of its initialization, under the separate preview identifier. The official updater plugin is not registered in private mode. Existing Sentry/private-frontend protections remain in place. Normal builds keep their original paths and updater behavior.

This isolates application-owned state, not the whole Windows operating system. Windows fonts, drivers and system services are still used normally. A user can deliberately export to or import from a chosen external location; the program is not a filesystem sandbox. Test existing work only from explicit copies. Native project editing is automatically saved, and text styles written at revision 2 cannot be opened by official 0.83.5. Uninstall must preserve PreviewData and never recursively erase user data.

### Model-download cache boundary

The pinned hf-hub dependency can initialize Hugging Face Xet shard, staging and logging caches outside its file cache even when cache_enabled(false) is set. In private mode, HuggingFaceFile::resolve now builds the same commit-pinned HTTPS /resolve URL with reqwest::Url path segments and uses the existing download::fetch implementation. This preserves retry/progress/range handling and private Store::file staging without constructing an Xet session. Model IDs, revisions, filenames and bytes are unchanged. Normal builds retain their existing hf-hub stream path. No process-wide or permanent Windows environment changes are made.

The installer rejects a destination containing koharu.exe unless that directory is already the registered preview installation. This guard also runs for silent installs. It preserves the exact-file uninstaller behavior, so PreviewData is retained.

## Text contour union

`koharu-renderer/src/text_renderer.rs` still extracts real glyph curves with the matched variable-font location, fallback geometry and synthetic font styles. After assembling the text's compound path, `union_contours()` flattens the curves at 0.02 local layout pixels with kurbo::flatten and passes the resulting rings to i_overlay's SimplifyShape with FillRule::NonZero. The union removes shared/internal boundaries while retaining counters with opposite winding. The original glyph curves still paint the fill. The new pure-Rust dependency is i_overlay 9.1.0 plus its lockfile dependencies; no existing dependency versions are intentionally changed.

This tolerance is below 0.1 display pixel at four-times scale. At extreme zoom or extreme layer scaling polygonal approximation can become visible; it is not an exact curve boolean. Union happens during retained text preparation, not on every canvas pan or zoom. It handles overlaps inside a glyph and between touching glyphs. Holes and self-intersections use nonzero winding throughout preparation and rasterization.

## Outside compositing

For an Outside contour outline with fully opaque text color, preparation emits a Center StrokePath at twice the requested width before the glyph fill. The fill covers the inward half. This gives continuous coverage at antialiased letter edges instead of blending a separately cut-away border against the same edge. Bounds continue to use the requested outside radius and join limit. No new prepared command or wire-format version is needed, so the existing Preview 1 WASM already understands the commands.

For text color alpha below 255, preparation keeps the real Outside StrokePath after the fill. The rasterizer's isolated DestOut operation prevents the stroke from appearing through translucent text or erasing the artwork below. The seam optimization is deliberately limited to opaque text; translucent antialiased boundaries still use the original compositing behavior.

## New text default

Manual text creation in commands/project.rs and automatic text-layer creation in pipeline/stages/detection.rs now set stroke_method to Some(Contour). The renderer and Inspector still interpret absent legacy values as Expansion so existing boxes keep their appearance. Existing explicit method settings remain editable. Agent style edits continue to use the typed typography update and preserve the resulting values.

## Verification / reproduction checkpoint

Source edits are written. Both focused private-profile tests passed, including separate copied executables and reloading their own saved settings. The contour union, antialiased-seam/transparent/translucent-fill pixel regression, and prepared-packet round-trip checks passed. The final Windows x64 optimized build and package verification passed. The isolated Preview 2 portable ZIP and setup are the corrected trial downloads. The runtime/build toolchain from Preview 1 is still available locally, but it must not be the only recovery source. Save a clean source checkpoint after meaningful changes and keep FEATURE_PROGRESS.md current.

Apply the Preview 2 cumulative patch to the exact original archive, or start from Preview 1 source and reproduce the changed functions described above. Use the private build environment from BUILD_PREVIEW.md, strip SENTRY_* and NEXT_PUBLIC_SENTRY_* values, preserve tauri/custom-protocol and Windows +crt-static with staticVCRuntime=false. Target Preview-2 output names so Preview 1 remains frozen. Windows GUI behavior remains a user acceptance check until actually run on Windows.

### CEF provenance

The official https://cef-builds.spotifycdn.com/index.json was reachable in this environment. Its Windows64 minimal entry for cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_windows64_minimal.tar.bz2 has SHA-1 be54bbaf3222191dec59ffd9c9ec7e8a7f44a26d. The complete local archive's computed SHA-1 matches that official entry. The runtime member comparison is repeated during packaging.

CLAUDE_REVIEW_FINDINGS.md preserves the supplied external static review. Its broader upstream security findings were not independently re-audited or changed in this milestone; no upstream report, message or patch was submitted.

## Focused commands

The private profile is selected at compile time. Set KOHARU_PRIVATE_PREVIEW=1 while running the profile test so the copied-executable cases run (the normal build keeps its normal profile). Use:

```text
cargo test -p koharu-config preview::tests -- --nocapture
cargo test -p koharu-renderer --lib text_renderer::tests::contour_ -- --nocapture
cargo test -p koharu-renderer --lib text_renderer::tests::rendered_text_survives_prepared_packet_round_trip -- --exact
cargo fmt --all --check
git diff --check
```

Five focused top-level tests passed: two profile tests, two contour regressions and one prepared-packet round trip. The private-profile persistence test also launches three copied-executable child processes. The pixel fixture places real glyphs using their contour bounds so the antialiased edges and solid interiors are inside the image, and checks alpha 0, 96 and 255. A first fixture positioned most glyphs outside the image; its coverage assertion caught this, and the fixture placement was corrected without weakening the assertions.

On this Linux build host, fontconfig development metadata and shared-library links were staged privately for the renderer tests. Native Windows source compiled without these Linux packages. The attempted fontconfig dlopen environment switch did not match fontique's static API and was discarded; it is not a source or package change. No dependency version was downgraded. A linker warning about unavailable Microsoft SDK PDB files affects debug metadata; the optimized Windows build succeeded.

Preview 2 does not change UI components, the IPC schema or the prepared-command format. It reuses the private production frontend and generated WASM from Preview 1, whose existing Center/Outside StrokePath commands understand the corrected preparation. Rebuilding from scratch can still regenerate both through the Windows script. That script now produces both the installer and portable ZIP, targets Preview-2 names, and refuses to clear a staging directory containing PreviewData.

The remaining feature requirements are preserved in FEATURE_REQUESTS.md. Keep them on hold after this corrected trial delivery.

## Final delivery and hold

The installer directory contains exactly the expected 242 runtime files plus normal NSIS installer internals. All runtime file contents match staging; the portable ZIP's 242 entries match by size/SHA256, all 10 PE files are complete x64 binaries, and all 237 CEF runtime/notice members match the officially indexed archive. No separate MSVC runtime imports were found. Local extraction again left two partial output files; reading those installer members as streams confirmed their full expected sizes and hashes. These were local extraction artifacts, not partial archive members.

A temporary .koharu.exe copy left in staging inflated the first installer candidate. The candidate was discarded. The final NSIS File command uses `/r /x ".*"`; the actual installer directory is checked against an exact expected set. Windows staging rejects dot-files, and both build and uninstall paths protect PreviewData. The final installer and portable ZIP are verified; actual Windows GUI launch and the user's problem font remain acceptance work.

Installer SHA256: 484090392d1272375996c29fd68aee5a56a8964e82a6b39b5e4de0fc0026f47f

Portable SHA256: b86ccf58052b197fa23543e7d5fa6ac6029438302e012211bc61a215dd6a50ac

The project is now on temporary hold. Do not start Perspective Transform, rich text or Mesh Warp until the user explicitly resumes. Preserve both Preview 1 and Preview 2 frozen sources/downloads and the original upload.
