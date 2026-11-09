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
  [string]$PythonPath = $env:ENGLISH_ASSIST_PYTHON,
  [ValidatePattern('^\d+\.\d+\.\d+(\.\d+)?$')][string]$Version
)
$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $PSScriptRoot 'frozen-payload.ps1')
$taskSourceVersion = Get-EnglishAssistSourceVersion $taskRepo
if (-not $Version) { $Version = $taskSourceVersion }
if ($Version -ne $taskSourceVersion) { throw 'Installer version must equal Cargo.toml workspace.package.version.' }
. (Join-Path $taskRepo 'tools\english-assist\enter-env.ps1')
Push-Location $taskRepo
try {
  # Embedded data must be reviewed/reproducible before Cargo reads include_bytes!.
  & (Join-Path $taskRepo 'tools\english-assist\build-english-bigrams.ps1') -PythonPath $PythonPath -Test:(-not $NoTest)
  if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw 'Rust is missing. Install/configure the MSVC Rust toolchain before building.'
  }
  $taskTargets = & rustup target list --installed
  if ($LASTEXITCODE -ne 0 -or $taskTargets -notcontains 'i686-pc-windows-msvc') {
    throw 'The i686-pc-windows-msvc Rust target is required.'
  }
  $taskStage = Join-Path $taskRepo ('dist\.english-assist-freeze-' + [Guid]::NewGuid().ToString('N'))
  $taskDistRoot = Join-Path $taskRepo 'dist'
  if ([IO.Directory]::Exists($taskDistRoot) -and
      (([IO.File]::GetAttributes($taskDistRoot) -band [IO.FileAttributes]::ReparsePoint) -ne 0)) {
    throw 'The build output directory must not be a reparse point.'
  }
  New-Item -ItemType Directory -Path $taskStage -Force | Out-Null
  & cargo build --release --locked -p retype-tsf -p retype-diag -p retype-settings-egui
  if ($LASTEXITCODE -ne 0) { throw 'Windows x64 build failed.' }
  Copy-EnglishAssistProductionGroup $taskRepo $taskStage 'x64'
  & cargo build --release --locked -p retype-learning --features broker
  if ($LASTEXITCODE -ne 0) { throw 'Learning broker build failed.' }
  Copy-EnglishAssistProductionGroup $taskRepo $taskStage 'learning'
  & cargo build --release --locked -p retype-ai --features service
  if ($LASTEXITCODE -ne 0) { throw 'AI helper build failed.' }
  Copy-EnglishAssistProductionGroup $taskRepo $taskStage 'ai'
  & cargo build --release --locked --target i686-pc-windows-msvc -p retype-tsf
  if ($LASTEXITCODE -ne 0) { throw 'Windows x86 TSF build failed.' }
  Copy-EnglishAssistProductionGroup $taskRepo $taskStage 'x86'
  & cargo run --release --locked -p retype-resource-build -- --english-only --out dist/english-dictionaries
  if ($LASTEXITCODE -ne 0) { throw 'Offline English vocabulary build/check failed.' }
  & (Join-Path $taskRepo 'tools\english-assist\build-english-lexicon.ps1') -PythonPath $PythonPath -Test:(-not $NoTest)
  Copy-EnglishAssistProductionGroup $taskRepo $taskStage 'support'
  Complete-EnglishAssistPayload $taskRepo $taskStage $Version
  $taskDist = Publish-EnglishAssistFrozenPayload $taskRepo $taskStage $Version
  # The production DLL/EXE files are now frozen. Tests/tools may rebuild target/.

  if (-not $NoTest) {
    & (Join-Path $PSScriptRoot 'tests\install-state.tests.ps1')
    & (Join-Path $PSScriptRoot 'tests\pending-renames.tests.ps1')
    & (Join-Path $PSScriptRoot 'tests\frozen-payload.tests.ps1')
    foreach ($taskShell in @('System32', 'SysWOW64')) {
      foreach ($taskMode in @('Missing','CoreOnly','Foreign')) {
        & (Join-Path $env:SystemRoot "$taskShell\WindowsPowerShell\v1.0\powershell.exe") -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tests\powershell-bootstrap.tests.ps1') -Mode $taskMode -CoreModulePath (Join-Path $PSHOME 'Modules')
        if ($LASTEXITCODE -ne 0) { throw "PowerShell bootstrap tests failed ($taskMode, $taskShell)." }
      }
      & (Join-Path $env:SystemRoot "$taskShell\WindowsPowerShell\v1.0\powershell.exe") -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tests\profile-interop.tests.ps1')
      if ($LASTEXITCODE -ne 0) { throw "Profile ABI/scope tests failed in $taskShell Windows PowerShell." }
      & (Join-Path $env:SystemRoot "$taskShell\WindowsPowerShell\v1.0\powershell.exe") -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tests\offline-resources.tests.ps1')
      if ($LASTEXITCODE -ne 0) { throw "Offline resource integrity tests failed in $taskShell Windows PowerShell." }
      & (Join-Path $env:SystemRoot "$taskShell\WindowsPowerShell\v1.0\powershell.exe") -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tests\manual-words-access.tests.ps1')
      if ($LASTEXITCODE -ne 0) { throw "Manual dictionary scope/ACL tests failed in $taskShell Windows PowerShell." }
      & (Join-Path $env:SystemRoot "$taskShell\WindowsPowerShell\v1.0\powershell.exe") -NoProfile -NonInteractive -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'tests\cleanup-process.tests.ps1')
      if ($LASTEXITCODE -ne 0) { throw "Cleanup process identity/race and own-TIP tombstone tests failed in $taskShell Windows PowerShell." }
    }
    & (Join-Path $PSScriptRoot 'tests\profile-interop.tests.ps1')
    & (Join-Path $PSScriptRoot 'tests\user-profile-diagnostics.tests.ps1')
    & cargo test --release --locked -p retype-tsf -p retype-english -p retype-engine -p retype-ai-core -p retype-ai -p retype-candidate-ui -p retype-learning -p retype-settings-egui --features retype-ai-core/service,retype-ai/service,retype-learning/broker
    if ($LASTEXITCODE -ne 0) { throw 'Core/TSF tests failed.' }
    & cargo test --release --locked --target i686-pc-windows-msvc -p retype-tsf
    if ($LASTEXITCODE -ne 0) { throw 'x86 TSF tests failed.' }
  }
  $null = Assert-EnglishAssistFrozenPayload $taskDist $Version
  if ($Installer) {
    & (Join-Path $PSScriptRoot 'repack-english-assist.ps1') -PayloadRoot $taskDist -IsccPath $IsccPath
  }
  Write-Output $taskDist
} finally {
  Pop-Location
}
