# Koharu feature requirements and scope

The user's supplied Koharu 0.83.5 source is the baseline. Implement in increasing effort order. The first two additions are the trial milestone; Perspective Transform, rich text and Mesh Warp remain on temporary hold until the user explicitly resumes them. Keep source checkpoints, a checklist and a reproducible implementation handoff after meaningful changes. Usage-limit interruptions must not leave completed work only in conversation state.

## 1. Favorite fonts — implemented

Manga lettering uses several recurring fonts for dialogue, narration, shouting, thoughts and character voices. Allow marking font families with stars. Favorites appear in a separate section above the complete alphabetical All Fonts section. They persist across application restarts and are available in every picker. Starring should not select a font or close the picker. Existing search, filters, previews and keyboard interaction should remain usable.

Retain the Preview 1 native fonts-section implementation. Claude's earlier 01-favorite-fonts.patch conflicts with it and should not be applied.

## 2. Contour outlines — implemented, with Preview 2 corrections

Offer an alternative to the original outward glyph-mask expansion, addressing rectangular protrusions and shape distortion with some fonts. Use conventional glyph-contour outlines with Outside, Center and Inside placement; width/color controls and appropriate joins. Keep text editable and use the same prepared result for the canvas, PNG, crops and PSD cached pixels. The feature does not require a separately editable Photoshop stroke effect in PSD.

The requested Preview 2 corrections merge overlapping glyph contours before stroking, paint opaque Outside strokes behind the fill to avoid an antialias seam, and select Contour for new manual/detected text. Preserve existing boxes' appearance and provide Expansion as an explicit choice. Translucent text requires the true outside-only path so the border cannot fill its interior.

## Trial isolation — required for Preview 2

The user wants to keep official Koharu installed and avoid automatic access to its application-owned files or credentials. A different installation drive alone is insufficient. The private preview must use executable-relative PreviewData for settings, projects, downloaded models/runtimes, browser storage and logs. Use a distinct operating-system credential namespace and separate app identity. Do not import or migrate official data, register official updating or fall back to official folders when the preview directory is unwritable. Keep private-preview error reporting disabled. Preserve PreviewData on uninstall. Test existing work only from deliberate copies. Shared Windows fonts, drivers and system services remain normal operating-system resources; this is not an OS sandbox.

## 3. Perspective Transform — not started; on hold

- Operate on selected image/layer content or a pixel selection, without changing unrelated layers.
- Four independently draggable corners form a quadrilateral with a true projective transform, including perspective compression.
- Show live preview and an optional internal alignment grid.
- Provide reset, apply and cancel; cancellation leaves original content untouched.
- Support undo/redo and saved, reopenable, non-destructive editing.
- Preserve editable text where feasible, transparency and high-quality resampling.
- Integrate with existing placement, scale and rotation, preferably in a shared Warp/Transform toolbar group.

Existing four-point geometry is a starting point, not proof that projective sampling, bounds, export or text editing already work. All output paths must agree.

## 4. Rich text — not started; on hold

Allow selected ranges within one text box to use mixed fonts, bold, italic and colors. The current controls apply to the whole box; manually dividing dialogue into many boxes is too tedious. Introduce styled runs with coherent shaping/layout, selection/editing, save/load, undo/redo and exports. Keep semantic text and presentation separate. Do not implement this by permanently splitting each styled run into independent canvas boxes.

## 5. Mesh Warp — not started; on hold

- Show an adjustable rectangular mesh over selected layer content.
- Drag individual nodes for localized stretching, compression, bends and irregular deformation.
- Edit curved connecting lines, not only straight segments.
- Add points/divisions with double-clicks inside the mesh or along an edge.
- Select and move several points together.
- Support smooth nodes and sharp corners.
- Show live preview; provide reset, apply and cancel.
- Support undo/redo and saved, reopenable, non-destructive distortion.
- Preserve transparent backgrounds and use good resampling.
- Keep text editable where feasible and preserve existing positioning/scale/rotation behavior.
- Integrate into the main toolbar, potentially under the same Warp/Transform menu as Perspective.

Examples include arched text, bending an edge, stretching one corner and fitting lettering to an irregular surface. Local curved deformation is essential; ordinary resizing or skew alone does not meet this request.

## Acceptance still required

Focused checks and package verification do not replace the user's Windows trial. Record actual launch, original-data isolation, favorites persistence, the user's problematic font, outline positions/joins, save/reopen, undo/redo and canvas/export comparisons separately. Do not mark these manual checks complete without evidence.
