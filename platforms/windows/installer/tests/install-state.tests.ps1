# Pure policy tests. No registry, installation, process, credential or deletion APIs are called.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\install-state.ps1')
$script:Passed = 0
function Assert-Equal($Actual, $Expected, [string]$Name) {
  if ($Actual -ne $Expected) { throw "$Name expected $Expected, got $Actual" }
  $script:Passed++
}
function Assert-Denied([string]$Name, [scriptblock]$Action) {
  $denied = $false
  try { $null = & $Action } catch { $denied = $true }
  if (-not $denied) { throw "$Name was unexpectedly authorized" }
  $script:Passed++
}
function New-State {
  @{
    CurrentSid='S-1-5-21-111-222-333-1001'
    InstallRoot='C:\Program Files\EnglishAssist'
    PayloadDirectory='C:\Program Files\EnglishAssist\versions\0.1.0-20261107010101-123'
    ProgramFilesDirectory='C:\Program Files'
    Machine=[pscustomobject]@{
      ProductId='{F981F16C-7472-4F3F-99C5-7C0E47956624}'
      TipClsid='{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}'
      ActiveDir='C:\Program Files\EnglishAssist\versions\0.1.0-20261107010101-123'
      FreshInstall=1
    }
    Owner=$null
    HasUserData=$false
    ProfileListed=$false
    ProfileEnabled=$false
  }
}
function New-Owner([string]$Directory='C:\Program Files\EnglishAssist\versions\0.1.0-20261107010101-123') {
  [pscustomobject]@{
    ProfileOwnerSid='S-1-5-21-111-222-333-1001'
    ProfileInstallRoot='C:\Program Files\EnglishAssist'
    ProfileInstallDir=$Directory
    ProfileProductId='{F981F16C-7472-4F3F-99C5-7C0E47956624}'
  }
}
$state = New-State
Assert-Equal (Get-EnglishAssistCleanupMode @state) 'MachineOnly' 'Untouched first installation skips all user cleanup'
$state.Owner = New-Owner
$state.HasUserData = $true
$state.ProfileListed = $true
$state.ProfileEnabled = $true
Assert-Equal (Get-EnglishAssistCleanupMode @state) 'Full' 'Current original user can clean owned data'
$state.Owner = New-Owner 'C:\Program Files\EnglishAssist\versions\0.0.9-old'
Assert-Equal (Get-EnglishAssistCleanupMode @state) 'Full' 'Failed upgrade may retain same-user previous version'
$state.Owner = [pscustomobject]@{
  ProfileOwnerSid='S-1-5-21-111-222-333-1001'
  ProfileInstallDir='C:\Program Files\EnglishAssist\versions\0.0.9-legacy'
}
Assert-Equal (Get-EnglishAssistCleanupMode @state) 'Full' 'Legacy same-root user record remains recoverable'
Assert-Denied 'Alternate administrator' {
  $s=New-State; $s.Owner=New-Owner; $s.CurrentSid='S-1-5-21-111-222-333-1002'
  Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'No-owner upgrade' {
  $s=New-State; $s.Machine.FreshInstall=0; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'No-owner personal data' {
  $s=New-State; $s.HasUserData=$true; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'No-owner listed profile' {
  $s=New-State; $s.ProfileListed=$true; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'No-owner enabled profile' {
  $s=New-State; $s.ProfileEnabled=$true; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'Incomplete ownership metadata' {
  $s=New-State; $s.Owner=[pscustomobject]@{ ProfileInstallDir=$s.PayloadDirectory }; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'Other product metadata' {
  $s=New-State; $s.Machine.ProductId='{00000000-0000-0000-0000-000000000000}'; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'Other TIP class' {
  $s=New-State; $s.Machine.TipClsid='{7E4C9A21-5B38-4D2E-9F6A-1C0D8E7B4A52}'; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'Other active payload' {
  $s=New-State; $s.Machine.ActiveDir='C:\Program Files\EnglishAssist\versions\another'; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'User record outside product root' {
  $s=New-State; $s.Owner=New-Owner 'C:\Program Files\retype\versions\old'; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'User record with another explicit root' {
  $s=New-State; $s.Owner=New-Owner; $s.Owner.ProfileInstallRoot='C:\Windows'; Get-EnglishAssistCleanupMode @s
}
Assert-Denied 'User record with another product identifier' {
  $s=New-State; $s.Owner=New-Owner; $s.Owner.ProfileProductId='another-product'; Get-EnglishAssistCleanupMode @s
}
foreach ($invalidRoot in @('C:\','C:\Windows','C:\Program Files','C:\Program Files\EnglishAssist-other','D:\EnglishAssist','EnglishAssist','C:EnglishAssist')) {
  $script:InvalidRoot = $invalidRoot
  Assert-Denied "Invalid install root $invalidRoot" {
    $null=Assert-EnglishAssistLayout $script:InvalidRoot 'C:\Program Files\EnglishAssist\versions\v1' 'C:\Program Files'
  }
}
foreach ($invalidPayload in @('C:\Program Files\EnglishAssist','C:\Program Files\EnglishAssist\versions-other\v1','C:\Program Files\EnglishAssist\versions\..\other','C:\Program Files\EnglishAssist\versions\v1\x86','C:\Windows\versions\v1','versions\v1')) {
  $script:InvalidPayload = $invalidPayload
  Assert-Denied "Invalid payload $invalidPayload" {
    $null=Assert-EnglishAssistLayout 'C:\Program Files\EnglishAssist' $script:InvalidPayload 'C:\Program Files'
  }
}
$layout = Assert-EnglishAssistLayout 'c:\program files\englishassist\' 'C:\Program Files\EnglishAssist\versions\v1\' 'C:\Program Files'
Assert-Equal $layout.Payload 'C:\Program Files\EnglishAssist\versions\v1' 'Windows case and trailing slash normalization'
Write-Output "$script:Passed installation path/ownership policy cases passed; no system changes."

