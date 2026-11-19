# Build Booharu on Windows

The Windows installer does not require these developer tools. This page is for rebuilding the modified source.

1. Extract the entire source ZIP into a normal folder, for example `C:\Booharu-Source`. Do not build from inside the ZIP.
2. Install Rust 1.99.1 or newer (MSVC toolchain), Bun 1.3.14 or newer, LLVM 22.1.8 or newer (its libclang generates the native bindings, and Visual Studio 18's standard library refuses an older Clang), CMake, Ninja, and NSIS.
3. Install Visual Studio Build Tools with **Desktop development with C++** and the Windows SDK. Restart your terminal after installing tools. CMake, Ninja, Rust and Bun must be on PATH; set `LIBCLANG_PATH` to LLVM's `bin` folder if bindgen cannot find libclang.
4. Open PowerShell in the extracted source folder and run:

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\build-preview-windows.ps1
   ```

5. The script builds the interface, builds the Windows program with Cargo, stages it with its CEF runtime and notices, and creates `dist\Booharu-<version>-Setup.exe`, `dist\Booharu-<version>-Portable.zip` and `dist\SHA256SUMS.txt`. The version comes from `FORK_VERSION`.

The build follows Koharu's own (`cargo tauri build --no-bundle`), calling Cargo directly with Tauri's `custom-protocol` feature so the Tauri CLI isn't needed. Cargo builds with `--locked`, exactly the dependencies `Cargo.lock` holds, so the CEF runtime staged beside the program is the one it was built for; a `Cargo.toml` change that needs a new lock stops the build until `Cargo.lock` is updated. CEF downloads into `target\cef` on the first build. The script removes Sentry credentials and DSNs from its environment, so the program has no address to report to and the interface build uploads nothing, and it disables Next.js telemetry. Booharu's own name, identifier, settings folder (`~/.booharu`) and credential-store entry (`booharu`) come from the source, not from the script. No model weights are bundled.

Released builds come from the "Preview Windows build" workflow on GitHub Actions, and each release page names the run that built it. Do not treat an unbuilt source archive as an installer.
