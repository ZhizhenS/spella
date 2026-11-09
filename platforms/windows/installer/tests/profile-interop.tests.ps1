# ABI and scope tests only. Invalid identities are rejected before creating any COM object.
$ErrorActionPreference = 'Stop'
$taskSource = [IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\user-profile.ps1'))
$taskCode = [regex]::Match($taskSource, "(?s)Add-Type -TypeDefinition @'\r?\n(.*?)\r?\n'@").Groups[1].Value
if (-not $taskCode) { throw 'Profile interop source was not found.' }
Add-Type -TypeDefinition $taskCode
$script:Passed = 0
function Assert-Equal($Actual, $Expected, [string]$Name) {
  if ($Actual -ne $Expected) { throw "$Name expected $Expected, got $Actual" }
  $script:Passed++
}
function Assert-IdentityRejected([scriptblock]$Action, [string]$Name) {
  $rejected = $false
  try { $null = & $Action }
  catch {
    $exception = $_.Exception
    while ($exception) {
      if ($exception -is [ArgumentException]) { $rejected = $true; break }
      $exception = $exception.InnerException
    }
    if (-not $rejected) { throw }
  }
  if (-not $rejected) { throw "$Name did not reject an unrelated identity" }
  $script:Passed++
}
$is64 = [IntPtr]::Size -eq 8
$expectedSize = if ($is64) { 88 } else { 72 }
Assert-Equal ([Runtime.InteropServices.Marshal]::SizeOf([type][EnglishAssist.NativeProfile])) $expectedSize 'Native SDK structure size'
$expectedOffsets = @{
  dwProfileType=0; langid=4; clsid=8; guidProfile=24; catid=40; hklSubstitute=56
  dwCaps=$(if ($is64) {64} else {60})
  hkl=$(if ($is64) {72} else {64})
  dwFlags=$(if ($is64) {80} else {68})
}
foreach ($field in $expectedOffsets.Keys) {
  Assert-Equal ([Runtime.InteropServices.Marshal]::OffsetOf([type][EnglishAssist.NativeProfile], $field).ToInt32()) $expectedOffsets[$field] "Native SDK field $field"
}
$ownProfile = 'F395A724-DC88-4E75-B3D4-9063A552B8F4'
Assert-IdentityRejected { [EnglishAssist.UserInputProfile]::IsEnabled([uint16]0x0804, $ownProfile) } 'Legacy read language guard'
Assert-IdentityRejected { [EnglishAssist.UserInputProfile]::GetModernFlags([uint16]0x0409, [Guid]::Empty.ToString()) } 'Modern read profile guard'
Assert-IdentityRejected { [EnglishAssist.UserInputProfile]::SetEnabled([uint16]0x0804, $ownProfile, $true) } 'Enable language guard'
Assert-IdentityRejected { [EnglishAssist.UserInputProfile]::SetEnabled([uint16]0x0409, [Guid]::Empty.ToString(), $false) } 'Disable profile guard'
Assert-IdentityRejected { [EnglishAssist.UserInputProfile]::DeactivateOwnProfile([uint16]0x0804, $ownProfile) } 'Deactivate language guard'
Assert-IdentityRejected { [EnglishAssist.UserInputProfile]::DeactivateOwnProfile([uint16]0x0409, [Guid]::Empty.ToString()) } 'Deactivate profile guard'
Write-Output "$script:Passed profile ABI/scope cases passed for $([IntPtr]::Size * 8)-bit PowerShell; no COM profile APIs invoked."

