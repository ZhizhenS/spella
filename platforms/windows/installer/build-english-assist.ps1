<#
.SYNOPSIS
  Build the independent English Assist Windows development installer.
.DESCRIPTION
  Requires an existing Rust MSVC toolchain, x86 target, Visual C++ tools/SDK.
  -Installer additionally requires an existing Inno Setup 6 installation.
  This script never installs the product or modifies Windows keyboard settings.
#>
[CmdletBinding()]
param(
  [switch]$NoTest,
  [switch]$Installer,
  [string]$IsccPath,
  [ValidatePattern('^\d+\.\d+\.\d+(\.\d+)?$')][string]$Version = '0.1.0'
)
$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $taskRepo 'tools\english-assist\enter-env.ps1')
Push-Location $taskRepo
try {
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Rust is missing. Install/configure the MSVC Rust toolchain before building.'
  }
  $taskTargets = & rustup target list --installed
  if ($LASTEXITCODE -ne 0 -or $taskTargets -notcontains 'i686-pc-windows-msvc') {
    throw 'The i686-pc-windows-msvc Rust target is required.'
  }
  & cargo build --release --locked -p retype-tsf -p retype-diag -p retype-settings-egui
  if ($LASTEXITCODE -ne 0) { throw 'Windows x64 build failed.' }
  & cargo build --release --locked -p retype-learning --features broker
  if ($LASTEXITCODE -ne 0) { throw 'Learning broker build failed.' }
  & cargo build --release --locked -p retype-ai --features service
  if ($LASTEXITCODE -ne 0) { throw 'AI helper build failed.' }
  & cargo build --release --locked --target i686-pc-windows-msvc -p retype-tsf
  if ($LASTEXITCODE -ne 0) { throw 'Windows x86 TSF build failed.' }
  & cargo run --release --locked -p retype-resource-build -- --english-only --out dist/english-dictionaries
  if ($LASTEXITCODE -ne 0) { throw 'Offline English vocabulary build/check failed.' }
  if (-not $NoTest) {
    & cargo test --release --locked -p retype-tsf -p retype-english -p retype-engine -p retype-ai-core -p retype-ai -p retype-candidate-ui -p retype-learning --features retype-ai-core/service,retype-ai/service,retype-learning/broker
    if ($LASTEXITCODE -ne 0) { throw 'Core/TSF tests failed.' }
    & cargo test --release --locked --target i686-pc-windows-msvc -p retype-tsf
    if ($LASTEXITCODE -ne 0) { throw 'x86 TSF tests failed.' }
  }
  $taskDist = Join-Path $taskRepo 'dist\english-assist'
  New-Item -ItemType Directory -Path $taskDist -Force | Out-Null
  New-Item -ItemType Directory -Path (Join-Path $taskDist 'x86') -Force | Out-Null
  New-Item -ItemType Directory -Path (Join-Path $taskDist 'settings') -Force | Out-Null
  New-Item -ItemType Directory -Path (Join-Path $taskDist 'dictionaries') -Force | Out-Null
  foreach ($taskFile in @('retype_ime.dll','retype-diag.exe','retype-learning-host.exe','retype-ai-host.exe')) {
    Copy-Item -LiteralPath (Join-Path $taskRepo "target\release\$taskFile") -Destination $taskDist -Force
  }
  Copy-Item -LiteralPath (Join-Path $taskRepo 'target\i686-pc-windows-msvc\release\retype_ime.dll') -Destination (Join-Path $taskDist 'x86') -Force
  Copy-Item -LiteralPath (Join-Path $taskRepo 'target\release\retype-settings-egui.exe') -Destination (Join-Path $taskDist 'settings\retype.exe') -Force
  Get-ChildItem -LiteralPath (Join-Path $taskRepo 'dist\english-dictionaries') -Filter 'english-*.bin' |
    Copy-Item -Destination (Join-Path $taskDist 'dictionaries') -Force
  foreach ($taskFile in @('LICENSE','NOTICE.txt','assets\retype.ico','platforms\windows\installer\user-profile.ps1','platforms\windows\installer\cleanup-english-assist.ps1')) {
    Copy-Item -LiteralPath (Join-Path $taskRepo $taskFile) -Destination $taskDist -Force
  }
  Get-ChildItem -LiteralPath (Join-Path $taskRepo 'data\dict\raw\english') -Filter 'LICENSE-*' |
    Copy-Item -Destination $taskDist -Force
  $taskMetadata = [ordered]@{
    product = 'English Assist'
    version = $Version
    upstreamCommit = '902e35e06ee4f744320cf995c39b1f64693034cf'
    tipClsid = '{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}'
    englishProfile = '{F395A724-DC88-4E75-B3D4-9063A552B8F4}'
    languageId = '0409'
    note = 'Build output; application compatibility must be tested separately.'
  }
  $taskMetadata | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $taskDist 'build.json') -Encoding UTF8
  if ($Installer) {
    if (-not $IsccPath) {
      $taskCandidates = @(
        'C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
        'C:\Program Files\Inno Setup 6\ISCC.exe',
        (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe')
      )
      $IsccPath = $taskCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
    }
    if (-not $IsccPath -or -not (Test-Path -LiteralPath $IsccPath)) {
      throw 'Inno Setup 6 is required to create the installer; pass -IsccPath.'
    }
    & $IsccPath "/DRepoRoot=$taskRepo" "/DMyAppVersion=$Version" (Join-Path $PSScriptRoot 'english-assist.iss')
    if ($LASTEXITCODE -ne 0) { throw 'English Assist installer compilation failed.' }
    $taskSetup = Join-Path $taskRepo "dist\english-assist-$Version-windows-x64-setup.exe"
    $taskHash = (Get-FileHash -LiteralPath $taskSetup -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText($taskSetup + '.sha256',
      $taskHash + '  ' + [IO.Path]::GetFileName($taskSetup) + [Environment]::NewLine,
      [Text.UTF8Encoding]::new($false))
    Write-Output $taskSetup
  }
  Write-Output $taskDist
} finally {
  Pop-Location
}
