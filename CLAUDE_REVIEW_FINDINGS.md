# Koharu: Security Audit and Feature Review

Prepared October 24, 2026, for a personal fork of [koharu-rs/koharu](https://github.com/koharu-rs/koharu).

| Item | Detail |
| --- | --- |
| Baseline | Koharu v0.83.5, upstream commit `af54817` |
| Reviewed | Upstream source, ChatGPT's "Typesetting Preview 1" source checkpoint and handoff documents, and the Preview 1 portable Windows build |
| Method | Static code review only. Nothing was compiled or run here: this environment cannot download Rust crates, npm packages or the CEF runtime. |

## Summary

- **Koharu itself is reasonably safe.** The security audit found no critical or high-severity issues. Three medium issues are listed in section 1.
- **ChatGPT's favorite-fonts feature is solid.** Use it in place of the earlier Claude patch.
- **ChatGPT's contour outlines are well designed but have three issues** (section 3):
  - Inside and Center strokes draw lines where letter shapes overlap.
  - Outside strokes can show a faint seam between text and outline.
  - New text still defaults to the old Expansion method.
- **Projects saved by the preview cannot be opened by official Koharu.** Back up `Documents\Koharu` before using the preview on real work.
- **The portable build checks out.** Its contents are as expected, and native crash reporting really is disabled. The CEF runtime checksum still needs your own confirmation (section 4).

---

## 1. Security audit of upstream Koharu

**Scope.** About 180,000 lines of Rust plus the Next.js editor UI, reviewed by hand:

- the Tauri desktop shell and its commands;
- file import (CBZ/ZIP, RAR, PDF), export and project storage;
- the runtime downloader and translation providers;
- the AI agent and its sign-in, and secrets storage;
- CI and release workflows;
- Rust dependencies, checked against the RustSec advisory database.

JavaScript dependencies could not be scanned, because the npm advisory service was unreachable from this environment.

### Findings

| # | Severity | Finding | Location |
| --- | --- | --- | --- |
| M1 | Medium | Crash reporting to Sentry is always on, sends personal data, and has no opt-out | `crates/koharu/src/sentry.rs` |
| M2 | Medium | GPU runtime libraries are downloaded and loaded with no hash or signature check | `crates/koharu-runtime/src/runtime/packages/` |
| M3 | Medium | The release job holds signing keys while running third-party actions pinned only to version tags; every dependency bump is auto-merged | `.github/workflows/` |
| L1 | Low | Lockfile behind on fresh advisories: rustls 0.23.45, rustls-webpki 0.103.15, zip 4.6.1 and ctutils 0.4.2 | `Cargo.lock` |
| L2 | Low | Gemini and Google Cloud API keys are sent in the URL query string | `koharu-translator/src/remote/gemini.rs`, `google_cloud.rs` |
| L3 | Low | No size limit when importing archives, so a "zip bomb" can exhaust memory and crash the app | `koharu-app/src/commands/import/` |
| L4 | Low | Content Security Policy disabled in the editor webview | `crates/koharu/tauri.conf.json` |
| L5 | Low | Text on manga pages can steer the AI agent, which can delete pages (undoable) | `koharu-app/src/commands/agent/host.rs` |
| I1 | Info | "Sign in with Codex" reuses the OAuth client ID of OpenAI's own Codex CLI | `koharu-agent/src/codex/auth.rs` |
| I2 | Info | Several unmaintained crates: paste, fxhash, ttf-parser, zvariant, atomic-polyfill | `Cargo.lock` |

### M1: Always-on crash reporting

The README says models run locally "to keep your data private." Even so, official builds report to Sentry on every launch, with `send_default_pii(true)` and session tracking, and there is no setting to turn it off. Reports can include:

- your IP address;
- OS and hardware details;
- file paths, which often contain your username and manga file names;
- up to 16 KB of an AI provider's error response.

The project's privacy notice discloses this; what's missing is consent.

**Preview 1 status:** fixed for your build. Sentry is compiled out (verified in section 4).

### M2: Unverified native libraries

On first use, Koharu downloads llama.cpp, libtorch and stable-diffusion builds from GitHub release tags, then loads them into the process with no checksum. The main app is signed and its updates are verified, but these libraries bypass that chain. Anyone able to replace a release file in those repositories could run code on every new install.

Some downloads are better protected:

- CUDA wheels are version-pinned on PyPI, where files can't be replaced.
- Hugging Face models are pinned to exact commits.
- Archive extraction is safe against path tricks.

**Fix:** embed an expected SHA-256 for each runtime file and check it before use.

### M3: Release pipeline

The release job holds the Apple signing certificate, the updater signing key and SignPath tokens. It runs third-party GitHub Actions pinned only to version tags, which can be re-pointed. On top of that, Dependabot updates are approved and merged automatically, daily, for every update including major versions. A hijacked upstream package could therefore reach the release job without human review.

**Fix:**

- pin actions to commit SHAs;
- add a Dependabot cooldown;
- limit auto-merge to patch updates.

### What upstream does well

- **Secrets:** API keys and tokens live in the OS keychain, are wiped from memory after use, and are redacted in logs.
- **Models and updates:** models are pinned to exact commits. Updates are signature-checked and install only after you confirm.
- **File handling:** archives are read in memory, so no path traversal. Project names are validated, including Windows reserved names, and export file names are sanitized.
- **Permissions:** the UI gets minimal Tauri permissions, with no filesystem or shell access.
- **Unsafe code:** confined to the native-library bindings and GPU probing.

---

## 2. Feature plan and ranking

| Rank | Feature | Effort | Status |
| --- | --- | --- | --- |
| 1 | Favorite fonts | Low | Done (ChatGPT, Preview 1) |
| 2 | Contour outlines | Moderate | Done, with fixes needed (section 3) |
| 3 | Perspective Transform | Substantial, but easier than it looks | Not started |
| 4 | Rich text | High | Not started |
| 5 | Mesh Warp | Very high | Not started |

What the code shows about the remaining features:

- **Perspective:** layer geometry is already stored as a polygon of points, typically four corners. Most of the work is rendering through a projective transform and adding corner handles.
- **Rich text:** a text box's content is a single plain string. Mixed fonts or colors within a box need a new data model for styled runs, which touches saving, layout, shaping, editing and export.
- **Mesh Warp:** needs everything Perspective needs, plus curved grid deformation, editable divisions and seamless rendering.

### Time estimates for features 1 and 2

| Who | Favorites | Outlines |
| --- | --- | --- |
| Developer who knows Koharu | 2–4 hours | 2–4 days |
| Developer new to Koharu | 1 day | 1–2 weeks |
| AI, writing the code | 15–30 minutes | 1–2 hours |

Expect about half a day of elapsed time end to end with builds, mostly waiting. Most of ChatGPT's 2h 46m went to:

- Windows SDK setup;
- the first full compile;
- recovering from an interruption;
- repairing two truncated runtime DLLs;
- packaging.

That is a reasonable time for the work.

---

## 3. Review of ChatGPT's Preview 1 source

### Favorite fonts: approved

- **Storage:** favorites go in their own `fonts` section of Koharu's settings file, through two native commands. `get_font_favorites` reads the list; `set_font_favorite` (`fonts.rs:30`) changes it, validates names and saves before returning.
- **Picker:** a shared query keeps every font picker in sync and serializes rapid toggles. Starring doesn't select the font or close the picker. Search and filters apply to both sections.
- **Tests:** dedicated font picker tests cover starring, ordering, removal, filtering, persistence and keyboard focus.
- **Versus the Claude patch:** better. The Claude patch saved favorites by rewriting the whole preferences object. **Use ChatGPT's version and discard `01-favorite-fonts.patch`;** the two conflict.
- **Minor:** if a save fails, the in-memory list keeps the change until restart. This is harmless.

### Contour outlines: well designed, needs three fixes

What's correct:

- The outline is a new rendering command (`StrokePath`) understood by both the desktop renderer and the browser canvas. Canvas, PNG, layer crops and PSD therefore stay identical.
- It uses the real glyph curves, including the variable-font axis position and fallback fonts.
- Outside placement isolates the stroke in its own layer before cutting its inner half away, so it can't damage artwork underneath.
- Bounds are padded correctly for each position and corner type. The miter-limit math checks out, so thick strokes won't be clipped in exports.
- Synthetic bold and italic are matched.
- Old projects open in the preview. The new settings use the save format's built-in versioning (`Typography` is now revision 2).

#### Issue 1 (high): Inside and Center strokes show lines where shapes overlap

`text_contours()` (`text_renderer.rs:415`) collects every glyph's raw outline into one path. The path is then stroked as is (`frame.rs:514`). Overlapping shapes are never merged first, so wherever two shapes overlap, the stroke is drawn *inside* the letter. This happens with:

- letters built from overlapping strokes, common in variable fonts and many CJK fonts;
- neighbouring letters that touch or overlap, typical of comic lettering fonts with tight or negative spacing.

Outside placement is unaffected, because the inner half of the stroke is erased.

**How to see it:** use a variable font ("VF" or "Variable" in the name) or a tightly spaced comic font. Choose Inside or Center with a contrasting color, and look for thin lines through the middle of letters.

**Fix:** merge all glyph outlines into one shape (a path union, nonzero fill) before stroking. Koharu's geometry library (kurbo) has no boolean operations, so this needs a small pure-Rust polygon library. One approach: flatten the curves at about 0.1 px tolerance, union them, then stroke the result. The loss of curve quality is invisible at that tolerance.

#### Issue 2 (medium): faint seam between Outside strokes and the text

For Contour mode, the text fill is drawn first (`text_renderer.rs`, contour branch). The Outside stroke is drawn afterwards, with its inner half removed using `Compose::DestOut` (`frame.rs:539–547`).

Along each letter edge, both the fill and the cut-away stroke are only partially opaque, because of anti-aliasing. A little of whatever is behind the text leaks through, peaking around 25% at half-covered pixels. The result:

- invisible on a plain white or black speech bubble;
- possibly a thin hairline between letters and outline on sound effects over dark or busy artwork.

Photoshop avoids this by drawing the outside stroke *behind* the fill.

**Fix:** when the text color is fully opaque (nearly always), draw the doubled-width stroke *before* the fill and skip the cut-away. Keep the current isolated cut-away only for semi-transparent text.

#### Issue 3 (medium): new text defaults to the old Expansion method

Missing settings resolve to Expansion in three places:

- the renderer (`renderer.rs:1464`);
- the Inspector (`Inspector.tsx:197`);
- new text and auto-detected text, which create `stroke_method: None` (`project.rs`, `detection.rs`).

Every new or translated text box therefore uses the method that causes the block artifacts, unless you change it by hand.

**Fix:** set `stroke_method: Some(Contour)` when creating text, or add a preference for the default outline method. Existing boxes keep their current look.

#### Trade-off to be aware of

In Contour mode, glyph hinting is turned off so the fill lines up exactly with the outline (`text_renderer.rs:128`). Small dialogue text may look very slightly softer than with Expansion. This is not a bug.

---

## 4. Portable build check

| Check | Result |
| --- | --- |
| Contents | 242 files: `koharu.exe`, the standard Chromium/CEF runtime (`libcef.dll`, `bootstrap.exe`/`bootstrapc.exe`, DirectX/Vulkan DLLs, `.pak` resources, locales), licenses and the install guide |
| Unexpected files | None: no scripts, installers or shortcuts |
| Crash reporting | No Sentry reporting address is compiled into `koharu.exe`; native crash reporting is off |
| Updater | The official update address is still embedded, but the preview hides the update check that uses it |
| Separate app identity | `rs.koharu.typesetting-preview` is present |
| CEF runtime | From `cef_binary_154.0.34+g14c5a08+chromium-154.0.8037.98_windows64_minimal.tar.bz2`; recorded SHA-1 `be54bbaf3222191dec59ffd9c9ec7e8a7f44a26d` |

**Still needed:** the official CEF index wasn't reachable from here. To confirm the runtime yourself:

1. Open [cef-builds.spotifycdn.com](https://cef-builds.spotifycdn.com/index.html).
2. Find **154.0.34 → Windows 64-bit → Minimal Distribution**.
3. Check that its SHA-1 matches the value above.

To check the installer before running it, use PowerShell:

```powershell
Get-FileHash .\Koharu-Typesetting-Preview-1-Setup.exe
# Expected: C89C4B5B414A761EABC43CB180A9BD2966D0861629C430C32D866C9D71986EA5
```

---

## 5. Before and while you trial the preview

### Compatibility

- **Projects saved by the preview cannot be opened by official Koharu.** The text style format moved to revision 2. The preview reads your old projects, but once it saves one, official 0.83.5 can't open it.
- **Both versions use the same `Documents\Koharu` folder.** Opening and editing a working chapter in the preview converts it.
- **Back up `Documents\Koharu` first.**
- **Don't run both versions at once.** They have separate app identities, so Windows allows it, but both would edit the same project files.
- **Settings are shared.** Both read `.koharu\config.toml` in your user folder. Official Koharu rewrites only its own sections when saving, so preview favorites survive switching back and forth. Other setting changes apply to both.
- **Downloaded models are shared,** so the preview won't re-download them.
- **Uninstalling the preview** removes only the program, not projects or settings.

### Trial checklist

**Favorites**

- [ ] Starring a font keeps the current font selected and leaves the picker open.
- [ ] Starred fonts appear under Favorites, above All Fonts.
- [ ] Favorites persist after restarting the preview.

**Outlines**

- [ ] The font that caused rectangular protrusions looks clean with Contour.
- [ ] Outside, Center and Inside each work; Round, Sharp and Bevel corners each work.
- [ ] **Issue 1:** Inside and Center on a variable or tightly spaced font show no lines through letters.
- [ ] **Issue 2:** Outside on dark artwork shows no hairline between text and outline.
- [ ] **Issue 3:** check which method a new text box starts with.
- [ ] Thick outlines near the edge of a text box aren't clipped in PNG export.

**Projects**

- [ ] Saving, closing and reopening keeps the outline settings.
- [ ] Undo and Redo work after changing an outline setting.

**Startup**

- [ ] If the canvas is blank or black on startup, suspect the runtime files before the new features.

---

## 6. Recommended next steps

1. Back up `Documents\Koharu`, then run the trial checklist above.
2. Fix the three outline issues as a patch on ChatGPT's source:
   - the default method and the seam fix are small;
   - the overlap merge is the larger change.
3. Then resume with Perspective Transform, the next feature and the one closest to existing code.
4. Optionally, upstream report M2 and M3 (privately first) to the Koharu maintainers.

### Long-term

This is now a fork that changes a core save format. If upstream Koharu changes text styling in a future release, merging will need care. Keep the patch files and handoff documents somewhere safe.
