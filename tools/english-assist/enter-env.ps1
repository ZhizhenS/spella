# Dot-source this script before building. It changes only the calling process.
$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$taskLocalToolchain = Join-Path (Split-Path $taskRepo -Parent) 'toolchain'
if (Test-Path -LiteralPath (Join-Path $taskLocalToolchain 'cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $taskLocalToolchain 'cargo'
    $env:RUSTUP_HOME = Join-Path $taskLocalToolchain 'rustup'
    $env:Path = "$env:CARGO_HOME\bin;$env:Path"
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Rust is missing. Install Rust stable MSVC, then retry.'
}
$taskVswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$taskVsRoot = $null
if (Test-Path -LiteralPath $taskVswhere) {
    $taskVsRoot = & $taskVswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
}
if (-not $taskVsRoot) {
    throw 'MSVC x64/x86 Build Tools are missing. Install Desktop development with C++ and Windows SDK from https://visualstudio.microsoft.com/visual-cpp-build-tools/ .'
}
$taskDeveloperShell = Join-Path $taskVsRoot 'Common7\Tools\Launch-VsDevShell.ps1'
if (-not (Test-Path -LiteralPath $taskDeveloperShell)) {
    throw 'The Visual Studio developer shell is incomplete.'
}
& $taskDeveloperShell -Arch amd64 -HostArch amd64 -SkipAutomaticLocation
# The VS developer shell rebuilds PATH from the registered environment. Restore
# this project's private Rust toolchain after it initializes the MSVC paths.
if (Test-Path -LiteralPath (Join-Path $taskLocalToolchain 'cargo\bin\cargo.exe')) {
    $env:CARGO_HOME = Join-Path $taskLocalToolchain 'cargo'
    $env:RUSTUP_HOME = Join-Path $taskLocalToolchain 'rustup'
    $env:Path = "$env:CARGO_HOME\bin;$env:Path"
}
if (-not (Get-Command cl.exe -ErrorAction SilentlyContinue)) {
    throw 'MSVC was detected but cl.exe is unavailable in its developer shell.'
}
Set-Location -LiteralPath $taskRepo
Write-Host "English Assist build environment ready: $taskVsRoot"
