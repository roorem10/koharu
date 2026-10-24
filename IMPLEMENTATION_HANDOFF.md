# Koharu Typesetting Preview 2: implementation and reproduction guide

Updated October 24, 2026. Baseline: the user's original `koharu-main.zip`, Koharu 0.83.5.

## Current version

Preview 2 contains favorite fonts, contour outlines, the requested outline corrections and isolated application data. Preview 1 downloads and source remain preserved. The detailed Preview 2 implementation is included below and also supplied as PREVIEW2_IMPLEMENTATION.md. FEATURE_REQUESTS.md preserves all five requested additions and their requirements.

## Scope and hold status

This milestone implements **favorite fonts**, **contour-based text outlines**, the requested rendering corrections and **separate preview application data**. The remaining three additions are on temporary hold after this corrected trial delivery. **Do not begin Perspective Transform, rich text, or Mesh Warp unless the user explicitly resumes that work.** Those three additions remain unimplemented.

This document explains the implementation sufficiently for a developer or another AI to recreate it. The complete modified source is the definitive reference. `FEATURE_PROGRESS.md` records verification and outstanding user acceptance; `PREVIEW_INSTALL.md` explains how to try the build; `BUILD_PREVIEW.md` explains the Windows rebuild. All paths below are relative to the extracted source root.

The frozen source archive also contains `Koharu-Typesetting-Preview-2.patch` beside its `koharu-main` folder. That patch is the exact text-file difference against the original supplied archive, including tests, translations, preview packaging and this documentation. Generated WASM and temporary build files are excluded from the patch. Rebuild WASM from the Rust source.

## 1. Architecture that these changes preserve

The React/Next.js interface invokes typed Tauri commands. Rust owns application preferences, scene state, project changes and preparation for rendering. Typography belongs to `koharu-scene`; `koharu-renderer` shapes text and prepares portable drawing commands; `koharu-rasterizer` compiles those commands into Vello scenes. The browser presents the prepared frame through the `koharu-canvas` WASM module and WebGPU.

One prepared frame must feed the canvas, PNG output, layer crops and PSD cached pixels. Implementing an outline only with CSS, a browser canvas effect, or an export-only filter would break that invariant. The new stroke is therefore a portable prepared command understood by both native rendering and WASM.

Favorites are application preferences rather than project edits. Outline choices are text-layer typography and use the existing undoable project edit path.

## 2. Favorite fonts

### Native persistence and typed commands

Add the existing workspace `koharu-config` dependency to `crates/koharu-app/Cargo.toml`. Cargo updates the `koharu-app` dependency entry in `Cargo.lock`; this does not introduce a new third-party package or change dependency versions.

In `crates/koharu-app/src/commands/fonts.rs`, add this private preference type:

```rust
#[derive(Default, Deserialize, Serialize)]
#[serde(default)]
struct FontPreferences {
    favorites: BTreeSet<String>,
}
```

Load it with `koharu_config::load::<FontPreferences>("fonts")`. This uses the existing native configuration manager and its durable save behavior. Store stable font **family names**, not a font's current index in the dropdown. The set removes duplicates and gives deterministic returned data. A favorite can remain in preferences while that font is temporarily unavailable.

Add two commands with the existing `#[tauri::command]` and `#[specta::specta]` annotations:

| Rust command | Arguments | Result |
| --- | --- | --- |
| `get_font_favorites` | none | Complete `Vec<String>` of saved favorites |
| `set_font_favorite` | `family_name: String`, `favorite: bool` | Complete updated `Vec<String>` |

The setter trims the name and rejects an empty value, more than 4,096 bytes, or an embedded NUL. It obtains a native write guard, inserts/removes the name, collects the updated names, calls `preferences.save()?`, and returns the collected list only after saving succeeds.

**Important ownership detail:** `ConfigWrite::save` consumes its guard. Collect the return value before calling `save`; do not try to read the moved guard or implement rollback by accessing it afterward.

Register both commands in `crates/koharu-app/src/commands/mod.rs` beside the existing font commands. Run the actual Rust binding generator:

```sh
cargo run -p koharu-app --bin generate
```

It regenerates `packages/bridge/src/protocol.ts` with `getFontFavorites()` and `setFontFavorite(familyName, favorite)`. Change Rust definitions and regenerate; do not maintain handwritten parallel bridge declarations.

### Shared query and mutation state

In `packages/koharu/lib/queries.ts`, add `fontFavoritesKey = ['font-favorites'] as const` and `useFontFavorites(enabled = true)`.

The hook reads through `call(commands.getFontFavorites)`. Its setter calls `call(commands.setFontFavorite, family, favorite)` and replaces the cached list with the complete native result on success. All instances use mutation key `fontFavoritesKey` and scope `{ id: 'font-favorites' }`, so changes across selectors are serialized and use shared data. `useIsMutating` supplies a shared busy flag. Pending/error loading also disables favorite toggles, preventing a misleading edit when the saved state is unknown.

The hook is enabled when the picker is open. Existing font loading and preview queries stay with their existing owners.

### Picker grouping, virtualization and accessibility

Update `packages/koharu/components/controls/FontPicker.tsx`:

1. Read favorites through the shared hook. Normalize names using the picker's existing normalization convention before comparing them.
2. Keep the current search and font filters. Build both sections from the resulting filtered family list.
3. When matching favorites exist, emit a Favorites heading and those families first, followed by an All Fonts heading and the complete filtered alphabetical list. Favorites also remain available in All Fonts.
4. Represent the virtualized list as heading rows and family rows. Use 24-pixel heading estimates and the existing 38-pixel family rows. Use stable, section-specific keys such as `favorite:<normalized-name>` and `all:<normalized-name>` so duplicate appearances are distinct virtual rows.
5. Start the list at the top when favorites exist. Scroll to the top when asynchronous preference loading introduces that section. With no favorites, retain the existing selected-font positioning behavior.
6. Put selection and star actions in separate sibling buttons. Do not nest a star button inside the font-selection button. Give the star a localized accessible label identifying its family.
7. Clicking a star invokes only the favorite mutation. It must neither change the chosen font nor close the popover.
8. Focus the search input with `preventScroll` after toggling. A favorite row can disappear from the virtualized section when unstarred; returning focus to a stable input avoids losing keyboard focus with the removed DOM node.

Preserve the existing font previews and list sizing. Review a long family name at both normal and compact window widths.

### Localization and regression coverage

Add the favorite headings/action labels, along with outline labels described below, to every `packages/koharu/public/locales/<locale>/translation.json`: `ar`, `en-US`, `es-ES`, `fr-FR`, `ja-JP`, `ko-KR`, `pt-BR`, `ru-RU`, `tr-TR`, `wuu`, `zh-CN`, and `zh-TW`.

`packages/koharu/tests/components/font-picker.test.tsx` exercises starring without selection/closure, favorite ordering, removal, filtering, restoring saved backend data after remount, and stable keyboard focus. `tests/setup.ts` adds a `scrollTo` mock for the test DOM. The native configuration manager supplies real persistence; the picker tests mock native command results. An actual restart on Windows remains an acceptance check.

## 3. Contour-based outlines

### Durable typography and every consumer

In `crates/koharu-scene/src/components/layers.rs`, introduce revisioned, serializable, typed enums:

```rust
pub enum StrokeMethod { Expansion, Contour }
pub enum StrokePosition { Outside, Center, Inside }
pub enum StrokeJoin { Round, Miter, Bevel }
```

Their wire names use `#[serde(rename_all = "snake_case")]`. Export them through both `components.rs` and the crate root `lib.rs`.

Bump `Typography` from revision 1 to revision 2 and append these fields, each annotated with `#[revision(start = 2)]`:

```rust
pub stroke_method: Option<StrokeMethod>,
pub stroke_position: Option<StrokePosition>,
pub stroke_join: Option<StrokeJoin>,
```

Keep existing numeric color and width representations. No new string color format or extension-map convention is necessary.

Update **all** constructors, copies and external views, rather than only the main Inspector path:

| File | Required work |
| --- | --- |
| `crates/koharu-app/src/commands/project.rs` | Add typed fields to the application Typography DTO; initialize new text; carry fields into scene updates and back into project views; update fixtures. |
| `crates/koharu-app/src/commands/agent/host.rs` | Add optional Agent schema enums/fields and explicit mappings into scene enums. |
| `crates/koharu-pipeline/src/stages/detection.rs` | Initialize all three fields to `None` for detected text. |
| `crates/koharu-scene/src/tests.rs` | Update Typography fixtures. |
| `crates/koharu-renderer/src/renderer.rs` | Update fixtures and resolve the three options into rendering stroke options. |
| `packages/koharu/tests/components/editor-components.test.tsx` | Update the typed UI fixture. |

Search for every `Typography {` constructor and every `stroke_width` mapping to find omissions. Regenerate the TypeScript protocol after the Rust mappings are complete. The renderer resolves absent legacy values to **Expansion**, **Outside**, and **Round**. Newly created manual and detected text explicitly sets `Some(StrokeMethod::Contour)` in project.rs and detection.rs. Existing boxes retain their choices and appearance; Expansion remains selectable.

The Inspector continues using the existing project update command. The new fields therefore travel with the normal project patch, inverse and undo/redo path; no separate outline history stack is introduced. Windows save/reopen and Undo/Redo still need the user's acceptance trial.

### Extract real glyph curves

Extend `StrokeOptions` in `crates/koharu-renderer/src/text_renderer.rs` with method, position and join. Make `TextRenderer::render` return `Result<Option<Rect>>`: extraction can fail, and a contour may extend beyond layout bounds.

For a visible, positive-width Contour stroke:

1. Iterate the already shaped `LayoutRun`: its lines, glyph IDs, actual fallback font for each glyph, advances and offsets. Do not reshape the string a second time.
2. Use each glyph font's Skrifa reference and `outline_glyphs().get(GlyphId::new(...))`.
3. Draw with `DrawSettings::unhinted(Size::new(layout.font_size), glyph.font.location())`. This preserves the current variable-font location.
4. Implement `ContourPen` for `OutlinePen`, collecting move, line, quadratic, cubic and close operations into a `kurbo::BezPath`. This supports actual font curves instead of tracing a raster mask.
5. Match existing synthetic styling. The small synthetic-bold expansion matches Vello's synthetic fill weight; it is not the new border algorithm. Apply synthetic skew, the font-to-screen Y flip, baseline, accumulated pen advances, glyph offsets, padding and baseline shift in the same coordinate system as the fill.
6. Normalize each glyph path's overall winding before combining fallback faces. Opposite dominant winding between fonts must not cancel the clipping region. Reverse all subpaths together when needed, preserving the relative winding of holes.
7. Flatten the assembled contours at 0.02 layout pixels using kurbo::flatten and nonzero-union them with i_overlay 9.1.0. Stroke the resulting visible boundary, removing internal edges where glyph components or neighbouring glyphs overlap. Preserve counters with opposite winding.
8. For fully opaque text with an Outside outline, record a Center StrokePath at twice the requested width before the glyph fill; the fill hides the inward half without an antialias seam. For Center/Inside and translucent Outside, record the stroke after the fill with its ordinary placement.
9. Draw the regular glyph fill from the original font curves with hinting disabled for this mode. Keep the original editable text and font resources. The merged outline is a subpixel polygon approximation; it is not a curve boolean.

If there is no drawable outline, retain the fill and return no additional contour bounds. Keep the Expansion branch's existing dilated border followed by ordinary fill behavior.

### Portable prepared command and packet version

In `crates/koharu-rasterizer/src/prepared.rs`:

- Add `PreparedSceneCommand::StrokePath(PreparedStrokePath)`.
- Add `PreparedStrokePath { path: PreparedPath, width: f64, position: StrokePosition, join: StrokeJoin }`.
- Keep independent serializable rasterizer position/join enums so portable display data does not depend on the scene crate.
- Validate the prepared path and require finite, strictly positive width.
- Set `PREPARED_FRAME_MANIFEST_VERSION` to **5**. The separate resource-format version remains **3**.
- Export the new types from `crates/koharu-rasterizer/src/lib.rs`.

The `PreparedPath` format retains its fill rule, affine transform, RGBA color and curve-element capability. Preview 2 text outlines use the merged polygon boundary. Building the feature from the original source requires rebuilding `koharu-canvas` WASM and the frontend to understand StrokePath. Preview 2 changes only preparation and reuses the same version-5 commands, so the already built private Preview 1 frontend/WASM can present its corrected commands without a wire-format update.

### Vello alignment and transparent compositing

In `crates/koharu-rasterizer/src/frame.rs`, dispatch the new command to `draw_stroke`. Compile the portable curve elements into a `BezPath`, convert position/join to Vello types, and use a miter limit of **4.0**.

The width control is the visible width for one-sided strokes:

| Position | Vello centered stroke width | Additional operation |
| --- | --- | --- |
| Center | `w` | Draw normally. |
| Inside | `2w` | Clip to the original filled contour. |
| Outside | `2w` | Opaque text: paint behind the fill. Translucent text: isolate and remove the inward half. |

For translucent or transparent Outside text, push an isolated normal layer bounded by the contour's inflated bounds, draw the stroke, then use an inner `Compose::DestOut` layer to remove the original filled contour. Pop both layers. The removal must operate inside the isolated stroke group: using destination-out directly on the main scene would erase the text or unrelated artwork underneath.

Honor `PreparedPath.fill` for clipping/removal, including NonZero and EvenOdd. Ordinary recorded text contours use NonZero. This is especially important for counters/holes and reusable prepared drawing data.

For Inside, push a clip layer with that same fill rule, draw the doubled stroke, and pop it. Center is an ordinary conventional stroke. Round/Miter/Bevel map directly to Vello joins; the UI calls Miter **Sharp**.

### Bounds, caching and exports

Calculate actual contour bounds and expand them by `w` for Outside, `w/2` for Center, and zero for Inside. Apply the conservative miter multiplier where relevant, transform the bounds, and return them. In `render_descriptor`, union these with the existing layout bounds when setting `local_bounds`. Keep the existing Expansion padding path.

Otherwise an outline may look correct on an unrestricted canvas but be cut off in a cropped layer or export. Do not enlarge or reshape the original layout box to simulate an outline.

The same prepared frame reaches the canvas, PNG, crops and PSD cached pixels. This milestone **does not add a separately editable Photoshop stroke effect to PSD**. Text stays editable in Koharu; the shared cached visual result includes its stroke.

### Inspector controls

In `packages/koharu/components/editor/Inspector.tsx`, add localized method, position and corner selectors, reusing existing width/color controls. Default nullable values to expansion/outside/round. Disable the position and corner selectors in Expansion mode.

Explicitly render the localized selected labels inside `SelectValue`. Merely giving menu items translated labels left the closed select showing raw enum values during review. Remove the fixed `h-48` typography section height so the added controls remain reachable at compact sizes.

## 4. Private preview and installation changes

These supporting changes make a safe, reviewable trial build; they are separate from the two editing features.

### Isolated application data

With compile-time KOHARU_PRIVATE_PREVIEW=1, koharu-config::preview resolves PreviewData beside current_exe and prepares writable settings, projects, packages, browser and logs directories. It never falls back to official directories or migrates official data. Config, ProjectLibrary and Store use this root; desktop startup sets CEF, webview, logs and window state to private paths, rejects the official app identifier and skips registering its updater. Secrets use a separate operating-system service name. Model HTTP downloads bypass Xet's user-global cache while preserving commit pins. Detailed ownership, paths, tests, installer guards and reproduction are included in the Preview 2 section below.

### Build and runtime reporting

Automatic approval review blocked an earlier unsanitized frontend build because Sentry could transmit source maps/build data. The provided private path prevents that disclosure:

| File | Private preview behavior |
| --- | --- |
| `packages/koharu/next.config.ts` | When `NEXT_PUBLIC_KOHARU_PREVIEW=1`, return the plain application configuration **before dynamically importing** `withSentryConfig`. Disabling uploads after constructing the plugin is insufficient for this path. |
| `packages/koharu/instrumentation-client.ts` | Skip Sentry initialization and router capture in preview mode. |
| `packages/koharu/app/global-error.tsx` | Skip Sentry exception capture in preview mode. |
| `crates/koharu/src/sentry.rs` | Return `Option<ClientInitGuard>`; return `None` when compile-time `KOHARU_PRIVATE_PREVIEW=1`. |
| `packages/koharu/app/providers.tsx` | Hide the official Updater in preview mode. |

Normal upstream behavior remains available when preview flags are unset. Every build for this delivery sets both preview flags, sets `NEXT_TELEMETRY_DISABLED=1`, and removes `SENTRY_*` and `NEXT_PUBLIC_SENTRY_*` environment variables. Do not reuse the earlier unsanitized pipeline.

### Windows build and installer

`preview/tauri.preview.conf.json` supplies the separate identifier `rs.koharu.typesetting-preview`, product name and disabled updater artifacts. It also sets `build.windows.staticVCRuntime=false`. The build explicitly uses Rust's `-C target-feature=+crt-static`; disabling Tauri's separate runtime override prevents it from requesting dynamic UCRT while cargo-xwin requests static UCRT.

`scripts/build-preview-windows.ps1` handles prerequisites, private environment, generation, WASM/frontend build, native release, runtime staging, notices and NSIS. **Direct Cargo release builds must include `--features tauri/custom-protocol`**; a release profile alone does not select Tauri's embedded production frontend.

`preview/installer.nsi` creates a per-user, unsigned setup in a separate default program folder. It stages only `.exe`, `.dll`, `.pak`, `.bin`, `.dat` and `.json` runtime files, copies `locales`/`swiftshader` when present, and excludes temporary dot-files. It includes the executable, CEF runtime/locales, licenses and install guide, creates Start menu entries and registers an uninstaller. Before packaging, `Assert-CompleteWindowsBinary` checks the MZ/PE signature, x64 machine value, section-table bounds and every raw section extent against the file length. This rejects incomplete extracted DLLs even if the downloaded archive hash was correct. The build generates an `UNINSTALL_INCLUDE` containing the exact installed file paths. The uninstaller deletes those files and uses nonrecursive directory removal, preserving unrelated files if an existing folder was chosen.

The registry string must be correctly quoted:

```nsis
WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\KoharuTypesettingPreview" "UninstallString" '"$INSTDIR\Uninstall.exe"'
```

Compile NSIS with `/WX` on Windows or `-WX` on Linux so string/packaging warnings fail before producing a trial installer. Include original MIT/Apache licenses and CEF `LICENSE.txt`/`CREDITS.html`, renamed to distinguish them in the installation folder. No model weights, credentials, SDK import libraries, PDBs or Rust build caches belong in the runtime package.

## 5. Reproduce from the original source

Use the exact original 0.83.5 archive as the starting point. Preserve a copy. Its SHA256 is `58bb374c356e8a67715f74ccfc9ba62a8ba4b58089f4ce1277a8ca4484271b8f`. The patch also adds this guide and progress/build documents.

1. Extract the original source into a new folder.
2. Copy the companion patch outside that folder. From the source root, run `git apply --check <path-to-patch>` and then `git apply <path-to-patch>`. A Git checkout is useful for inspection; `git apply` can also operate on the extracted files.
3. Alternatively, follow sections 2–4 manually, using the complete modified files as the exact reference. Update authoritative Rust owners before UI types.
4. Install the tool versions/prerequisites in `BUILD_PREVIEW.md`. Use the supplied lockfiles; do not silently downgrade dependencies to make an old toolchain compile them.
5. Regenerate typed bindings, rebuild WASM, then build the private frontend and native application. The provided PowerShell script performs these steps on Windows.
6. Run the focused checks below and inspect the actual UI.
7. Stage only runtime files, generate the exact uninstall list, build the installer with warnings treated as errors, extract it, and compare every staged file with its extracted counterpart.

For a manual private build, set preview/reporting isolation first. The essential build order is:

```sh
cargo run -p koharu-app --bin generate
bun run --filter @koharu/bridge build
bun run --filter @koharu/app build
cargo build -p koharu --release --features tauri/custom-protocol
```

Those commands assume the private environment and Tauri preview configuration from the supplied PowerShell script. Without those settings they are not the provided private preview build.

If the optional `wasm-opt` download is unavailable, build the same Rust canvas with wasm-pack 0.15.0 and `--no-opt`. Keep target `web`, output name `koharu_canvas`, `--no-pack`, and output directory `packages/bridge/src/wasm`. Skipping optional post-optimization does not remove the contour implementation. Rebuild the frontend afterward so it includes the resulting WASM.

The generated protocol is included in the exact text patch because it was produced by the actual generator. Generated WASM is included in the full source checkpoint for recovery convenience, while rebuilding remains the reproducible path. Exclude `target`, `node_modules`, `.next`, `out`, `dist`, `.review`, `*.tsbuildinfo` and `next-env.d.ts` from source deliveries.

### Linux cross-build notes

The trial was cross-built for `x86_64-pc-windows-msvc` with Rust 1.97.1, Bun 1.3.14, LLVM/clang-cl/lld 22.1.8, cargo-xwin 0.23.1, and the official CEF 154.0.34 Windows archive. The normal Windows rebuild script does not require reproducing the Linux sysroot.

For another Linux build:

- Prepare the Microsoft CRT/SDK through `cargo xwin env --target x86_64-pc-windows-msvc` before compilation. Preserve its cache until the build finishes.
- Verify the official CEF archive hash against its index entry. Prepare the layout as `download-cef` does: Release files at the CEF root, Resources entries alongside them, plus include/cmake/libcef_dll and notices. Its `archive.json` records the verified archive's type, name and SHA1. Set `CEF_PATH` to that prepared root.
- Supply LLVM 22's resource directory through `CL_FLAGS`, which cargo-xwin applies to target C/C++ flags. Do not set global CFLAGS/CXXFLAGS to that option: host GCC rejects it.
- A small local clang-cl wrapper handles POSIX absolute input paths by inserting `--` where needed. It also enables `/clang:-msse4.1` **only** for libwebp vendor translation units ending in `sse41.c`. Their runtime dispatcher checks CPU support. Do not broaden the whole application's CPU requirement with a global SSE4.1 flag.
- If necessary, stage CMake's `libuv` and LLVM's `libz3` dependencies. These are build-environment repairs, not application dependency changes.
- If the runtime supplies an HTTP proxy and xwin cannot use its default SOCKS configuration, use the supplied HTTPS proxy through `ALL_PROXY`; never put actual proxy credentials/URLs into project files or documentation.
- Apply the same private flags, preview JSON, static CRT flag and production protocol feature. Use `cargo xwin build -p koharu --release --target x86_64-pc-windows-msvc --features tauri/custom-protocol`.

Validate extracted binary lengths/PE sections as well as the archive hash. In this session, extraction left `libcef.dll` and `dxcompiler.dll` truncated even though the archive was intact. Re-reading those two complete members from the same verified minimal archive restored valid files. No dependency version or compiled application code changed. All 237 CEF runtime/notice files were subsequently matched by size and SHA256 against members of the verified minimal archive, whose SHA1 is `be54bbaf3222191dec59ffd9c9ec7e8a7f44a26d`. The initial portable checkpoint was replaced after all runtime PE files parsed and every portable member matched staging by size and SHA256.

Save the validated staged portable runtime immediately after a successful native build, before lengthy NSIS compression. This resumed build does so automatically. Temporary toolchains and native binaries were cleared during a prior usage-limit interruption; the saved source checkpoint preserved both implementations. The portable and final installer checkpoints prevent another interruption from requiring the completed executable to be rebuilt.

## 6. Verification and acceptance

The Preview 1 implementation checks include native app/generator compilation and real binding generation; renderer checking; the focused native compositor pixel test; a real contour-glyph prepared-packet round-trip; 7 focused font/localization tests; an additional rerun of the 2 font tests after focus hardening; TypeScript; focused lint; Rust formatting; final WASM; and private production frontend builds. The private frontend rebuild passed again after restoration. The resumed Windows x64 optimized release build also passed; its only linker warning concerns unavailable Microsoft SDK PDB debug metadata, which does not require shipping those debugger files.

The UI review used actual FontPicker/Inspector components in a headless browser with a mocked native backend at 1100×780 and 900×600, including a long font name. It identified the selected-label and fixed-height issues described above. That review is not a claim of Windows desktop or final WebGPU acceptance. Temporary review screenshots/harnesses are not part of the delivery.

Focused reproduction commands, from the source root with dependencies available:

```sh
cargo check -p koharu-renderer
cargo test -p koharu-rasterizer --test compositor contour_stroke_alignment_preserves_transparency_and_background
cargo test -p koharu-renderer text_renderer::tests::rendered_text_survives_prepared_packet_round_trip
cargo fmt --all --check
bun run --filter @koharu/app test tests/components/font-picker.test.tsx tests/lib/localization.test.ts
bun x tsc --noEmit --project packages/koharu/tsconfig.json
```

Run the existing frontend linter on changed frontend files as well. Native tests use the normal debug profile and a usable GPU/software adapter where required. Full test suites, end-to-end tests and a Windows GUI launch were not claimed. Final release/package results are recorded in `FEATURE_PROGRESS.md`.

The compositor test protects Outside/Center/Inside pixel placement, transparent interiors, and preservation of artwork below an outside stroke. The renderer test protects real glyph contour recording and portable packet reconstruction. The picker tests protect interaction and shared saved-data behavior. These checks cover different boundaries; a green TypeScript check alone would not verify rendering or persistence.

Before accepting the milestone, the user should test actual Windows launch, favorite persistence across restart, the previously problematic font, each alignment/join, save/reopen, Undo/Redo and canvas/PNG comparison. Keep trial projects separate from files opened in older Koharu versions. PSD cached pixels share the rendering path, but editable Photoshop stroke effects are outside this milestone.

## 7. Files changed

Core font implementation: `crates/koharu-app/Cargo.toml`, `Cargo.lock`, `crates/koharu-app/src/commands/fonts.rs`, `crates/koharu-app/src/commands/mod.rs`, `packages/koharu/lib/queries.ts`, `packages/koharu/components/controls/FontPicker.tsx`.

Scene and command integration: `crates/koharu-scene/src/components/layers.rs`, `crates/koharu-scene/src/components.rs`, `crates/koharu-scene/src/lib.rs`, `crates/koharu-app/src/commands/project.rs`, `crates/koharu-app/src/commands/agent/host.rs`, `crates/koharu-pipeline/src/stages/detection.rs`, and generated `packages/bridge/src/protocol.ts`.

Rendering: `crates/koharu-renderer/src/renderer.rs`, `crates/koharu-renderer/src/text_renderer.rs`, `crates/koharu-rasterizer/src/prepared.rs`, `crates/koharu-rasterizer/src/frame.rs`, `crates/koharu-rasterizer/src/lib.rs`.

UI/locales: `packages/koharu/components/editor/Inspector.tsx` and the 12 locale files listed in section 2.

Tests/fixtures: `crates/koharu-rasterizer/tests/compositor.rs`, `crates/koharu-scene/src/tests.rs`, renderer/project fixtures in their implementation files, `packages/koharu/tests/components/font-picker.test.tsx`, `packages/koharu/tests/components/editor-components.test.tsx`, and `packages/koharu/tests/setup.ts`.

Private preview: `crates/koharu/src/sentry.rs`, `packages/koharu/next.config.ts`, `packages/koharu/instrumentation-client.ts`, `packages/koharu/app/global-error.tsx`, `packages/koharu/app/providers.tsx`, `preview/tauri.preview.conf.json`, `preview/installer.nsi`, `scripts/build-preview-windows.ps1`.

Documentation: `BUILD_PREVIEW.md`, `PREVIEW_INSTALL.md`, `FEATURE_PROGRESS.md`, `RESUME_WORK.md`, and this `IMPLEMENTATION_HANDOFF.md`.

## 8. Detailed Preview 2 implementation and reproduction

Updated October 24, 2026. This is a focused follow-up to Preview 1. The frozen Preview 1 source and downloads are preserved. The user has not tested either build yet. Perspective Transform, rich text and Mesh Warp remain on hold.

### Requested changes

The user supplied Koharu-Review-Findings.md from Claude. Its feature review identified overlapping contour strokes, a possible antialiased seam in Outside outlines, and the legacy default for new text. The user also requested a test build that cannot automatically read or change official Koharu application data. Installing on another drive alone does not provide that separation.

### Data ownership

`crates/koharu-config/src/preview.rs` owns the private preview root. The existing private-build flag KOHARU_PRIVATE_PREVIEW=1 is compiled into the application; no user environment setup or launcher is required. The root is always a PreviewData directory beside the executable, resolved with current_exe, independent of the working directory. It has settings, projects, packages, browser and logs subdirectories. Startup creates and checks these directories and performs a checked write/remove probe. An inaccessible root stops startup. It never falls back to Documents/Koharu, ~/.koharu, an old installation's store or another user's cache. Existing settings/projects are not migrated.

`koharu-config::path()` chooses PreviewData/settings/config.toml. `ProjectLibrary::new()` chooses PreviewData/projects. `Store::root()` and desktop startup select PreviewData/packages for both downloaded model files and native inference packages. `koharu-secrets` selects the separate rs.koharu.typesetting-preview service in the operating-system credential store; credentials stay protected by Windows and do not travel in the portable folder. The new dependency from koharu-secrets to koharu-config has no cycle.

Desktop startup rejects the official app identifier when compiled as a private preview. It configures CEF's root_cache_path and the window's data_directory to PreviewData/browser, logs to PreviewData/logs, and window-state filename to an absolute path in PreviewData/settings. The window-state plugin can create an empty application-specific AppData directory as part of its initialization, under the separate preview identifier. The official updater plugin is not registered in private mode. Existing Sentry/private-frontend protections remain in place. Normal builds keep their original paths and updater behavior.

This isolates application-owned state, not the whole Windows operating system. Windows fonts, drivers and system services are still used normally. A user can deliberately export to or import from a chosen external location; the program is not a filesystem sandbox. Test existing work only from explicit copies. Native project editing is automatically saved, and text styles written at revision 2 cannot be opened by official 0.83.5. Uninstall must preserve PreviewData and never recursively erase user data.

#### Model-download cache boundary

The pinned hf-hub dependency can initialize Hugging Face Xet shard, staging and logging caches outside its file cache even when cache_enabled(false) is set. In private mode, HuggingFaceFile::resolve now builds the same commit-pinned HTTPS /resolve URL with reqwest::Url path segments and uses the existing download::fetch implementation. This preserves retry/progress/range handling and private Store::file staging without constructing an Xet session. Model IDs, revisions, filenames and bytes are unchanged. Normal builds retain their existing hf-hub stream path. No process-wide or permanent Windows environment changes are made.

The installer rejects a destination containing koharu.exe unless that directory is already the registered preview installation. This guard also runs for silent installs. It preserves the exact-file uninstaller behavior, so PreviewData is retained.

### Text contour union

`koharu-renderer/src/text_renderer.rs` still extracts real glyph curves with the matched variable-font location, fallback geometry and synthetic font styles. After assembling the text's compound path, `union_contours()` flattens the curves at 0.02 local layout pixels with kurbo::flatten and passes the resulting rings to i_overlay's SimplifyShape with FillRule::NonZero. The union removes shared/internal boundaries while retaining counters with opposite winding. The original glyph curves still paint the fill. The new pure-Rust dependency is i_overlay 9.1.0 plus its lockfile dependencies; no existing dependency versions are intentionally changed.

This tolerance is below 0.1 display pixel at four-times scale. At extreme zoom or extreme layer scaling polygonal approximation can become visible; it is not an exact curve boolean. Union happens during retained text preparation, not on every canvas pan or zoom. It handles overlaps inside a glyph and between touching glyphs. Holes and self-intersections use nonzero winding throughout preparation and rasterization.

### Outside compositing

For an Outside contour outline with fully opaque text color, preparation emits a Center StrokePath at twice the requested width before the glyph fill. The fill covers the inward half. This gives continuous coverage at antialiased letter edges instead of blending a separately cut-away border against the same edge. Bounds continue to use the requested outside radius and join limit. No new prepared command or wire-format version is needed, so the existing Preview 1 WASM already understands the commands.

For text color alpha below 255, preparation keeps the real Outside StrokePath after the fill. The rasterizer's isolated DestOut operation prevents the stroke from appearing through translucent text or erasing the artwork below. The seam optimization is deliberately limited to opaque text; translucent antialiased boundaries still use the original compositing behavior.

### New text default

Manual text creation in commands/project.rs and automatic text-layer creation in pipeline/stages/detection.rs now set stroke_method to Some(Contour). The renderer and Inspector still interpret absent legacy values as Expansion so existing boxes keep their appearance. Existing explicit method settings remain editable. Agent style edits continue to use the typed typography update and preserve the resulting values.

### Verification / reproduction checkpoint

Source edits are written. Both focused private-profile tests passed, including separate copied executables and reloading their own saved settings. The contour union, antialiased-seam/transparent/translucent-fill pixel regression, and prepared-packet round-trip checks passed. The final Windows x64 optimized build and package verification passed. The isolated Preview 2 portable ZIP and setup are the corrected trial downloads. The runtime/build toolchain from Preview 1 is still available locally, but it must not be the only recovery source. Save a clean source checkpoint after meaningful changes and keep FEATURE_PROGRESS.md current.

Apply the Preview 2 cumulative patch to the exact original archive, or start from Preview 1 source and reproduce the changed functions described above. Use the private build environment from BUILD_PREVIEW.md, strip SENTRY_* and NEXT_PUBLIC_SENTRY_* values, preserve tauri/custom-protocol and Windows +crt-static with staticVCRuntime=false. Target Preview-2 output names so Preview 1 remains frozen. Windows GUI behavior remains a user acceptance check until actually run on Windows.

#### CEF provenance

The official https://cef-builds.spotifycdn.com/index.json was reachable in this environment. Its Windows64 minimal entry for cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_windows64_minimal.tar.bz2 has SHA-1 be54bbaf3222191dec59ffd9c9ec7e8a7f44a26d. The complete local archive's computed SHA-1 matches that official entry. The runtime member comparison is repeated during packaging.

CLAUDE_REVIEW_FINDINGS.md preserves the supplied external static review. Its broader upstream security findings were not independently re-audited or changed in this milestone; no upstream report, message or patch was submitted.

### Focused commands

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

### Final delivery and hold

The installer directory contains exactly the expected 242 runtime files plus normal NSIS installer internals. All runtime file contents match staging; the portable ZIP's 242 entries match by size/SHA256, all 10 PE files are complete x64 binaries, and all 237 CEF runtime/notice members match the officially indexed archive. No separate MSVC runtime imports were found. Local extraction again left two partial output files; reading those installer members as streams confirmed their full expected sizes and hashes. These were local extraction artifacts, not partial archive members.

A temporary .koharu.exe copy left in staging inflated the first installer candidate. The candidate was discarded. The final NSIS File command uses `/r /x ".*"`; the actual installer directory is checked against an exact expected set. Windows staging rejects dot-files, and both build and uninstall paths protect PreviewData. The final installer and portable ZIP are verified; actual Windows GUI launch and the user's problem font remain acceptance work.

Installer SHA256: 484090392d1272375996c29fd68aee5a56a8964e82a6b39b5e4de0fc0026f47f

Portable SHA256: b86ccf58052b197fa23543e7d5fa6ac6029438302e012211bc61a215dd6a50ac

The project is now on temporary hold. Do not start Perspective Transform, rich text or Mesh Warp until the user explicitly resumes. Preserve both Preview 1 and Preview 2 frozen sources/downloads and the original upload.

## 9. Handoff rule

Deliver the first-two-features installer/portable build, source, this guide and the checklist. Then leave the project **on temporary hold**. Preserve the original upload and both frozen Preview 1 and Preview 2 sources. Use the updated Preview 2 downloads for the isolated trial. A future developer should inspect the saved checklist and the user's latest instruction before changing anything; the unimplemented backlog is not authorization to start it.
