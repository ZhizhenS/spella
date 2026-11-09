<# Repack an existing verified production freeze. Never builds/copies native binaries. #>
[CmdletBinding()]
param(
  [string]$PayloadRoot,
  [switch]$RefreshScripts,
  [switch]$VerifyOnly,
  [string]$IsccPath
)
$ErrorActionPreference = 'Stop'
$taskRepo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
. (Join-Path $PSScriptRoot 'frozen-payload.ps1')
if (-not $PayloadRoot) { $PayloadRoot = Join-Path $taskRepo 'dist\english-assist' }
$PayloadRoot = [IO.Path]::GetFullPath($PayloadRoot)
$taskVersion = Get-EnglishAssistSourceVersion $taskRepo
if ($RefreshScripts) { Update-EnglishAssistFrozenScripts $taskRepo $PayloadRoot $taskVersion }
$taskMetadata = Assert-EnglishAssistFrozenPayload $PayloadRoot $taskVersion
if ($VerifyOnly) { $taskMetadata; return }
if (-not $IsccPath) {
  foreach ($taskCandidate in @('C:\Program Files (x86)\Inno Setup 6\ISCC.exe',
      'C:\Program Files\Inno Setup 6\ISCC.exe', (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'))) {
    if ([IO.File]::Exists($taskCandidate)) { $IsccPath = $taskCandidate; break }
  }
}
if (-not $IsccPath -or -not [IO.File]::Exists($IsccPath)) { throw 'Inno Setup 6 is required; pass -IsccPath.' }
& $IsccPath "/DRepoRoot=$taskRepo" "/DPayloadRoot=$PayloadRoot" "/DMyAppVersion=$taskVersion" (Join-Path $PSScriptRoot 'english-assist.iss')
if ($LASTEXITCODE -ne 0) { throw 'English Assist installer compilation failed.' }
$taskSetup = Join-Path $taskRepo "dist\english-assist-$taskVersion-windows-x64-setup.exe"
$taskHash = Get-EnglishAssistPayloadHash $taskSetup
[IO.File]::WriteAllText(($taskSetup + '.sha256'),
  ($taskHash + '  ' + [IO.Path]::GetFileName($taskSetup) + [Environment]::NewLine), [Text.UTF8Encoding]::new($false))
Write-Output $taskSetup
