# Test the outline alpha correction on Windows

This build corrects how white and colored text/outline edges blend over page images. It retains the filled Contour outlines, font favorites, isolated preview data, and working startup fix. The canvas renderer and native export renderer have both been rebuilt.

## Update your working preview

Use **Koharu-Alpha-Fix-Executable-Update.zip** if the previous preview already runs on your PC. Its companion CEF/runtime files are unchanged.

1. Close Koharu Typesetting Preview completely.
2. Open your working preview folder:

   `D:\KoharuStartupTest\Koharu-Typesetting-Preview-2-Startup-Fix`

3. Rename its existing `koharu.exe` to **`koharu-before-alpha-fix.exe`**. Keep this backup. If that backup name already exists, choose a different backup name.
4. Download the executable update ZIP. Right-click it and choose **Extract All** into a temporary folder, such as a new folder in Downloads.
5. Open that extracted folder. Copy its new **`koharu.exe`** into your working preview folder from step 2.
6. Double-click **`koharu.exe` in your working preview folder** to launch the updated version.

Keep the existing **PreviewData** folder in place. Keeping the executable in the same directory preserves the preview's data location, favorites, and stored project paths. The update does not require developer tools. The original Koharu installation remains separate.

The canvas code is embedded in the new executable. There are no separate canvas files to copy.

## Fresh separate test

For a fresh trial, extract **Koharu-Typesetting-Preview-2-Alpha-Fix-Portable.zip** to a new folder and run the `koharu.exe` inside it. That folder gets its own PreviewData. Favorites and projects in the previous preview folder stay there.

## First visual test

Open the same blue test page and use:

- CCWildWords Regular, weight 400, font size **300**.
- Black lettering and white outline, width **24.5**.
- **Contour Stroke / Outside / Round**.

Export PNG at the original page size. Inspect it at **100% zoom** in the same viewer used for the Affinity comparison. Check the curved outer white edge around the S, G, O, and rounded letter ends. Compare the canvas at 100% as well.

Please put the original exported PNG in a ZIP before attaching it, so the comparison can use the original resolution.

Then check:

- White or colored lettering over the page image, including text with no outline.
- Expansion outlines, plus Contour Center/Inside and the other corner choices.
- Transparent/translucent text and an exported PSD layer over a contrasting background.
- Favorite fonts after closing and reopening the preview.

Automated checks reproduce and correct the compositing fault. Your exact Windows visual test still decides acceptance.

## Roll back

Close the preview. Remove or rename the new `koharu.exe`, then rename your backup to `koharu.exe`. Leave PreviewData in place.

## Included scope

This is the outline alpha correction and the small outline review fixes. Revised Windows patch **04** has not been supplied to this workspace and is **not included**. The later Work Orders have not been implemented.
