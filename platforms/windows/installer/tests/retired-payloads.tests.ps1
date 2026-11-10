# Retired payload pruning. Policy cases are pure; removal cases use a private
# temporary "Program Files" tree and never touch the real installation.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\install-state.ps1')
$script:Passed = 0
function Assert-Equal($Actual, $Expected, [string]$Name) {
  if ($Actual -ne $Expected) { throw "$Name expected $Expected, got $Actual" }
  $script:Passed++
}

$root = 'C:\Program Files\EnglishAssist'
$active = "$root\versions\0.6.1-20261110120000-1"
$previous = "$root\versions\0.6.0-20261109143156-791774"
$names = @('0.1.0-20261107191702-788314', '0.6.0-20261109143156-791774', '0.6.1-20261110120000-1', 'notes', '0.6.0-garbage', '..')
$retired = @(Get-EnglishAssistRetiredPayloads $root $active $previous $names 'C:\Program Files')
Assert-Equal ($retired -join ',') '0.1.0-20261107191702-788314' 'Only well-formed payloads other than active/previous are retired'
$retired = @(Get-EnglishAssistRetiredPayloads $root $active '' $names 'C:\Program Files')
Assert-Equal ($retired -join ',') '0.1.0-20261107191702-788314,0.6.0-20261109143156-791774' 'Without a previous payload only the active one is kept'
$retired = @(Get-EnglishAssistRetiredPayloads $root $active 'D:\Elsewhere\0.1.0-20261107191702-788314' $names 'C:\Program Files')
Assert-Equal $retired.Count 2 'A previous payload outside versions protects nothing'
$denied = $false
try { $null = Get-EnglishAssistRetiredPayloads 'C:\Other' $active '' $names 'C:\Program Files' } catch { $denied = $true }
Assert-Equal $denied $true 'A foreign install root is refused'

$temp = Join-Path ([IO.Path]::GetTempPath()) ('ea-prune-' + [guid]::NewGuid().ToString('N'))
try {
  $programFiles = Join-Path $temp 'Program Files'
  $versions = Join-Path $programFiles 'EnglishAssist\versions'
  foreach ($name in @('0.1.0-20261107191702-788314', '0.2.0-20261108021743-150876', '0.6.0-20261109143156-791774', '0.6.1-20261110120000-1', 'keep-me')) {
    $null = New-Item -ItemType Directory -Path (Join-Path $versions "$name\dictionaries") -Force
    [IO.File]::WriteAllText((Join-Path $versions "$name\retype_ime.dll"), 'x')
    [IO.File]::WriteAllText((Join-Path $versions "$name\dictionaries\english.bin"), 'x')
  }
  # A running application still has this retired payload's DLL loaded.
  $locked = [IO.File]::Open((Join-Path $versions '0.2.0-20261108021743-150876\retype_ime.dll'), 'Open', 'Read', 'Read')
  try {
    $report = Remove-EnglishAssistRetiredPayloads (Join-Path $programFiles 'EnglishAssist') (Join-Path $versions '0.6.1-20261110120000-1') (Join-Path $versions '0.6.0-20261109143156-791774') $programFiles
  } finally { $locked.Dispose() }
  Assert-Equal $report.Removed 1 'An unlocked retired payload is removed'
  Assert-Equal $report.Kept 1 'A loaded retired payload is kept for a later install'
  Assert-Equal (Test-Path (Join-Path $versions '0.1.0-20261107191702-788314')) $false 'Removed payload is gone'
  Assert-Equal (Test-Path (Join-Path $versions '0.2.0-20261108021743-150876\dictionaries\english.bin')) $true 'Locked payload stays intact, not partially deleted'
  foreach ($name in @('0.6.0-20261109143156-791774', '0.6.1-20261110120000-1', 'keep-me')) {
    Assert-Equal (Test-Path (Join-Path $versions $name)) $true "$name is preserved"
  }
} finally {
  Remove-Item -LiteralPath $temp -Recurse -Force -ErrorAction SilentlyContinue
}
Write-Output "$script:Passed retired payload cases passed; only a private temporary tree was modified."
