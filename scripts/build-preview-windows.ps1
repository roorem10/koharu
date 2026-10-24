param([switch]$SkipDependencies)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

function Invoke-Checked {
    param([string]$Program, [string[]]$Arguments)
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Program failed with exit code $LASTEXITCODE" }
}

function Assert-CompleteWindowsBinary {
    param([string]$Path)
    $stream = [System.IO.File]::OpenRead($Path)
    $reader = [System.IO.BinaryReader]::new($stream)
    try {
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5A4D) { throw "Invalid Windows binary: $Path" }
        $stream.Position = 0x3C
        $peOffset = $reader.ReadUInt32()
        if ([UInt64]$peOffset + 24 -gt $stream.Length) { throw "Truncated Windows binary: $Path" }
        $stream.Position = $peOffset
        if ($reader.ReadUInt32() -ne 0x4550 -or $reader.ReadUInt16() -ne 0x8664) { throw "Expected a Windows x64 binary: $Path" }
        $sectionCount = $reader.ReadUInt16()
        $stream.Position = $peOffset + 20
        $optionalSize = $reader.ReadUInt16()
        $sectionStart = [UInt64]$peOffset + 24 + $optionalSize
        if ($sectionStart + [UInt64]$sectionCount * 40 -gt $stream.Length) { throw "Truncated section table: $Path" }
        for ($index = 0; $index -lt $sectionCount; $index++) {
            $stream.Position = $sectionStart + $index * 40 + 16
            $rawSize = $reader.ReadUInt32()
            $rawOffset = $reader.ReadUInt32()
            if ($rawSize -gt 0 -and [UInt64]$rawOffset + [UInt64]$rawSize -gt $stream.Length) {
                throw "Incomplete extracted binary; restore it from the verified CEF archive: $Path"
            }
        }
    }
    finally { $reader.Dispose() }
}

foreach ($program in @('cargo', 'rustup', 'bun', 'cmake', 'ninja')) {
    if (-not (Get-Command $program -ErrorAction SilentlyContinue)) {
        throw "Missing $program. See BUILD_PREVIEW.md for the Windows build prerequisites."
    }
}
$makensis = Get-Command makensis -ErrorAction SilentlyContinue
if ($makensis) { $makensis = $makensis.Source }
else { $makensis = Join-Path ${env:ProgramFiles(x86)} 'NSIS\makensis.exe' }
if (-not (Test-Path $makensis)) { throw 'NSIS is missing. See BUILD_PREVIEW.md.' }

$env:NEXT_PUBLIC_KOHARU_PREVIEW = '1'
$env:KOHARU_PRIVATE_PREVIEW = '1'
$env:NEXT_TELEMETRY_DISABLED = '1'
Get-ChildItem Env: | Where-Object { $_.Name -like 'SENTRY_*' -or $_.Name -like 'NEXT_PUBLIC_SENTRY_*' } |
    ForEach-Object { Remove-Item ("Env:" + $_.Name) }
$env:CARGO_PROFILE_RELEASE_DEBUG = '0'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS = '-C target-feature=+crt-static'
$env:TAURI_CONFIG = Get-Content (Join-Path $root 'preview\tauri.preview.conf.json') -Raw

if (-not $SkipDependencies) {
    Invoke-Checked 'rustup' @('target', 'add', 'wasm32-unknown-unknown')
    Invoke-Checked 'bun' @('install', '--frozen-lockfile')
}
Invoke-Checked 'cargo' @('run', '-p', 'koharu-app', '--bin', 'generate')
Invoke-Checked 'bun' @('run', '--filter', '@koharu/bridge', 'build')
Invoke-Checked 'bun' @('run', '--filter', '@koharu/app', 'build')
# Direct Cargo builds must opt into Tauri's bundled frontend protocol.
Invoke-Checked 'cargo' @('build', '-p', 'koharu', '--release', '--features', 'tauri/custom-protocol')

$out = Join-Path $root 'dist'
$stage = Join-Path $out 'Koharu-Typesetting-Preview-2'
if (Test-Path (Join-Path $stage 'PreviewData')) {
    throw 'The staging folder contains preview user data. Move that complete preview folder to a safe location before rebuilding.'
}
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null
$runtime = Join-Path $root 'target\release'
$extensions = @('.exe', '.dll', '.pak', '.bin', '.dat', '.json')
Get-ChildItem $runtime -File | Where-Object { $_.Extension -in $extensions -and -not $_.Name.StartsWith('.') } | Copy-Item -Destination $stage -Force
foreach ($directory in @('locales', 'swiftshader')) {
    $path = Join-Path $runtime $directory
    if (Test-Path $path) { Copy-Item $path -Destination $stage -Recurse -Force }
}
Copy-Item (Join-Path $root 'LICENSE-MIT'), (Join-Path $root 'LICENSE-APACHE'), (Join-Path $root 'PREVIEW_INSTALL.md') -Destination $stage -Force

# CEF stores its third-party notices beside its build headers. Preserve them in the preview.
$cefRoot = $env:CEF_PATH
if (-not $cefRoot) {
    $cef = Get-ChildItem (Join-Path $root 'target\release\build') -Filter 'archive.json' -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.DirectoryName -match 'windows_x86_64' } | Select-Object -First 1
    if ($cef) { $cefRoot = $cef.DirectoryName }
}
if (-not $cefRoot) { throw 'Cannot locate CEF notices. Keep the CEF build directory and retry.' }
Copy-Item (Join-Path $cefRoot 'LICENSE.txt') -Destination (Join-Path $stage 'CEF-LICENSE.txt') -Force
Copy-Item (Join-Path $cefRoot 'CREDITS.html') -Destination (Join-Path $stage 'CEF-CREDITS.html') -Force

Get-ChildItem $stage -File | Where-Object { $_.Extension -in @('.exe', '.dll') } |
    ForEach-Object { Assert-CompleteWindowsBinary $_.FullName }
if (Get-ChildItem $stage -Recurse -File -Force | Where-Object { $_.Name.StartsWith('.') }) {
    throw 'Temporary dot-files are present in runtime staging. Remove them before packaging.'
}

# Remove only the files we install, even when an existing folder is chosen.
$uninstallInclude = Join-Path $out 'uninstall-preview-files.nsh'
$removals = @()
Get-ChildItem $stage -Recurse -File | ForEach-Object {
    $relative = $_.FullName.Substring($stage.Length + 1).Replace('$', '$$')
    $removals += 'Delete "$INSTDIR\' + $relative + '"'
}
Get-ChildItem $stage -Recurse -Directory | Sort-Object { $_.FullName.Length } -Descending | ForEach-Object {
    $relative = $_.FullName.Substring($stage.Length + 1).Replace('$', '$$')
    $removals += 'RMDir "$INSTDIR\' + $relative + '"'
}
Set-Content -Path $uninstallInclude -Value $removals -Encoding utf8

Invoke-Checked $makensis @('/WX', "/DRUNTIME_DIR=$stage", "/DOUTPUT_DIR=$out", "/DSOURCE_DIR=$root", "/DUNINSTALL_INCLUDE=$uninstallInclude", (Join-Path $root 'preview\installer.nsi'))
Compress-Archive -LiteralPath $stage -DestinationPath (Join-Path $out 'Koharu-Typesetting-Preview-2-Portable.zip') -Force
Write-Host "Installer created: $(Join-Path $out 'Koharu-Typesetting-Preview-2-Setup.exe')"
Write-Host "Portable ZIP created: $(Join-Path $out 'Koharu-Typesetting-Preview-2-Portable.zip')"
