# Build Booharu on Windows

The Windows installer does not require these developer tools. This page is for rebuilding the modified source.

1. Extract the entire source ZIP into a normal folder, for example `C:\Koharu-Preview-Source`. Do not build from inside the ZIP.
2. Install Rust 1.97.1 or newer (MSVC toolchain), Bun 1.3.14 or newer, LLVM 22.1.8 or newer, CMake, Ninja, and NSIS.
3. Install Visual Studio Build Tools with **Desktop development with C++** and the Windows SDK. Restart your terminal after installing tools. LLVM's `bin` folder, CMake, Ninja, Rust and Bun must be on PATH; set `LIBCLANG_PATH` to LLVM's `bin` folder if bindgen cannot find libclang.
4. Open PowerShell in the extracted source folder and run:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-preview-windows.ps1
   ```

5. The script regenerates typed bindings from Rust, builds the canvas and frontend, builds the Windows application, stages its CEF runtime and notices, and creates `dist\Booharu-<version>-Setup.exe`, `dist\Booharu-<version>-Portable.zip` and `dist\SHA256SUMS.txt`. The version comes from `FORK_VERSION`.

The preview uses its own application identifier and install folder. The build script sets `NEXT_PUBLIC_KOHARU_PREVIEW=1` and `KOHARU_PRIVATE_PREVIEW=1` to suppress the official updater, skip Sentry's build plugin, and skip browser/native Sentry initialization. It also disables Next.js telemetry and removes Sentry credentials/DSNs from the build process environment. The script explicitly enables Tauri’s production frontend protocol and statically links the Rust C runtime; the preview config disables Tauri’s separate runtime-linking override to avoid conflicting UCRT choices. Project files and native Koharu preferences use the application's existing locations. No model weights are bundled.

Build status and verification results are recorded in FEATURE_PROGRESS.md. Do not treat an unbuilt source archive as an installer.

## Isolated preview data

KOHARU_PRIVATE_PREVIEW=1 now also selects executable-relative PreviewData for application state and a separate OS credential service. Build with the preview app identifier; the private executable rejects the official identifier at startup. Do not ship PreviewData, credentials or downloaded model stores in the source/package. See PREVIEW2_IMPLEMENTATION.md for the complete changes from Preview 1.
