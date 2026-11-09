# Pure path policy and in-memory ACL definitions. Never call the initializer or Set-Acl.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot '..\manual-words-access.ps1')
$script:Passed = 0
$script:Local = 'C:\Users\Example\AppData\Local'
$script:Root = "$script:Local\EnglishAssist"
$script:Words = "$script:Root\words"
$script:File = "$script:Words\manual.json"
$script:Attributes = @{}
$script:Children = @()
$read = { param($Path)
  if ($script:Attributes.ContainsKey($Path)) { return $script:Attributes[$Path] }
  if ($Path -eq $script:Root -or $Path.StartsWith($script:Root + '\', [StringComparison]::OrdinalIgnoreCase)) { return 0 }
  return 0x10
}
$readChildrenCallback = { param($Path) $script:Children }
function Assert-Equal($Actual, $Expected, [string]$Name) {
  if ($Actual -ne $Expected) { throw "$Name expected $Expected, got $Actual" }
  $script:Passed++
}
function Assert-Rejected([scriptblock]$Action, [string]$Name) {
  $rejected = $false
  try { $null = & $Action } catch { $rejected = $true }
  if (-not $rejected) { throw "$Name was accepted" }
  $script:Passed++
}
function Reset-Policy { $script:Attributes = @{}; $script:Children = @() }
$layout = Get-EnglishAssistManualWordsLayout $script:Local $read
Assert-Equal $layout.Root $script:Root 'Product root'
Assert-Equal $layout.Words $script:Words 'Only words directory'
Assert-Equal $layout.File $script:File 'Absolute manual path'
Assert-Rejected { Get-EnglishAssistManualWordsLayout 'relative\Local' $read } 'Relative root'
Assert-Rejected { Get-EnglishAssistManualWordsLayout '\\host\share' $read } 'UNC root'
Assert-Rejected { Get-EnglishAssistManualWordsLayout 'C:\' $read } 'Drive root'
foreach ($path in @('C:\Users', $script:Local, $script:Root, $script:Words, $script:File)) {
  Reset-Policy; $script:Attributes[$path] = 0x410
  Assert-Rejected { Get-EnglishAssistManualWordsLayout $script:Local $read } "Reparse point at $path"
}
Reset-Policy; $script:Attributes[$script:Local] = 0
Assert-Rejected { Get-EnglishAssistManualWordsLayout $script:Local $read } 'Missing application data'
foreach ($path in @($script:Root, $script:Words)) {
  Reset-Policy; $script:Attributes[$path] = 0x80
  Assert-Rejected { Get-EnglishAssistManualWordsLayout $script:Local $read } 'File in directory position'
}
Reset-Policy; $script:Attributes[$script:File] = 0x10
Assert-Rejected { Get-EnglishAssistManualWordsLayout $script:Local $read } 'Directory in manual-file position'
Reset-Policy; $script:Attributes[$script:File] = 0x80
Assert-Equal ((Get-EnglishAssistManualWordsLayout $script:Local $read).File) $script:File 'Existing regular file preserved'
Reset-Policy; $script:Children = @($script:File, "$script:File.bak", "$script:Words\manual-123.tmp")
foreach ($path in $script:Children) { $script:Attributes[$path] = 0x80 }
Assert-Equal (@(Get-EnglishAssistManualWordsChildren $script:Words $readChildrenCallback $read).Count) 3 'Flat regular store files'
$script:Attributes[$script:File] = 0x400
Assert-Rejected { Get-EnglishAssistManualWordsChildren $script:Words $readChildrenCallback $read } 'Linked child'
Reset-Policy; $script:Children = @("$script:Words\nested"); $script:Attributes[$script:Children[0]] = 0x10
Assert-Rejected { Get-EnglishAssistManualWordsChildren $script:Words $readChildrenCallback $read } 'Nested directory propagation'
Reset-Policy; $script:Children = @("$script:Root\other.json")
Assert-Rejected { Get-EnglishAssistManualWordsChildren $script:Words $readChildrenCallback $read } 'Entry outside words'

$rules = @(Get-EnglishAssistManualWordsReadRules)
Assert-Equal $rules.Count 2 'Separate directory and file permissions'
$writeMask = [int][Security.AccessControl.FileSystemRights]'Write, Delete, ChangePermissions, TakeOwnership'
foreach ($rule in $rules) {
  Assert-Equal $rule.IdentityReference.Value 'S-1-15-2-1' 'Only ALL APPLICATION PACKAGES'
  Assert-Equal ([int]$rule.FileSystemRights -band $writeMask) 0 'No write/delete/ACL/owner permission'
  Assert-Equal $rule.AccessControlType ([Security.AccessControl.AccessControlType]::Allow) 'Read permission kind'
}
Assert-Equal $rules[0].InheritanceFlags ([Security.AccessControl.InheritanceFlags]::ContainerInherit) 'Directory traverse inheritance'
Assert-Equal $rules[0].PropagationFlags ([Security.AccessControl.PropagationFlags]::None) 'Directory permission applies here'
Assert-Equal ([int]$rules[0].FileSystemRights -band 0x20) 0x20 'Directory traverse'
Assert-Equal $rules[1].InheritanceFlags ([Security.AccessControl.InheritanceFlags]::ObjectInherit) 'File read inheritance'
Assert-Equal $rules[1].PropagationFlags ([Security.AccessControl.PropagationFlags]::InheritOnly) 'File rule inherited only'
Assert-Equal ([int]$rules[1].FileSystemRights -band 0x20) 0 'Files receive no execute permission'

# Exercise the same Set+Add operations against a DACL held solely in memory.
$acl = [Security.AccessControl.DirectorySecurity]::new()
$owner = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($owner,
  [Security.AccessControl.FileSystemRights]::FullControl, [Security.AccessControl.AccessControlType]::Allow))
$acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($rules[0].IdentityReference,
  [Security.AccessControl.FileSystemRights]::Write, [Security.AccessControl.AccessControlType]::Allow))
$acl.SetAccessRule($rules[0]); $acl.AddAccessRule($rules[1])
$actual = @($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]))
$packageRules = @($actual | Where-Object { $_.IdentityReference.Value -eq 'S-1-15-2-1' })
Assert-Equal $packageRules.Count 2 'Directory and file rules remain separate'
foreach ($rule in $packageRules) { Assert-Equal ([int]$rule.FileSystemRights -band $writeMask) 0 'Old explicit package write removed' }
Assert-Equal (@($actual | Where-Object { $_.IdentityReference.Value -eq $owner.Value }).Count) 1 'Other identities retained'
Write-Output "$script:Passed manual dictionary scope/ACL cases passed; no directories, files, registry or on-disk ACLs modified."
