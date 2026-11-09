# Pure pending-rename parser and path policy tests; no registry/files are changed.
$ErrorActionPreference='Stop'
. (Join-Path $PSScriptRoot '..\install-state.ps1')
$script:Passed=0
function Assert-True($Actual,[string]$Name) {
  if (-not $Actual) { throw "$Name failed" }
  $script:Passed++
}
function Assert-Denied([string]$Name,[scriptblock]$Action) {
  $denied=$false
  try { $null=& $Action } catch { $denied=$true }
  if (-not $denied) { throw "$Name was unexpectedly allowed" }
  $script:Passed++
}
$root='C:\Program Files\EnglishAssist'
$old=$root+'\versions\0.1.0-20261107191702-788314'
$new=$root+'\versions\0.1.0-20261107220000-222222'
function Entry([string]$Source,[string]$Destination='') { [pscustomobject]@{Source=$Source;Destination=$Destination} }
function Policy($Entries=@(),$Links=@(),[string]$New=$new,$Directories=@()) {
  Test-EnglishAssistPendingRenamePolicy -InstallRoot $root -NewPayloadDirectory $New -Entries @($Entries) -ReparsePoints @($Links) -ProgramFilesDirectory 'C:\Program Files' -VerifiedDirectories @($Directories)
}
Assert-True (Policy) 'Empty queue'
Assert-True (Policy @(Entry ($old+'\retype_ime.dll'))) 'Old x64 deletion'
Assert-True (Policy @(Entry ($old+'\x86\retype_ime.dll'))) 'Old x86 deletion'
Assert-True (Policy @(Entry ('\??\'+$old+'\retype_ime.dll'))) 'Native NT prefix'
Assert-True (Policy @(Entry ('\\?\'+$old+'\x86\retype_ime.dll'))) 'Extended native prefix'
Assert-True (Policy @(Entry (($old+'\RETYPE_IME.DLL').ToLowerInvariant()))) 'Case insensitive Windows paths'
Assert-True (Policy @((Entry 'C:\Windows\Temp\other.tmp'),(Entry 'D:\Other\a' 'D:\Other\b'),(Entry ($old+'\retype_ime.dll')))) 'Unrelated system operations ignored'
Assert-True (Policy @(Entry 'C:\Program Files\EnglishAssist-other\retype_ime.dll')) 'Sibling product is unrelated'
Assert-True (Policy @(Entry ($root+'\versions\1.2.3.4-20240229010101-1\retype_ime.dll'))) 'Four-part version and valid leap date'
Assert-True (Policy @(Entry ('*1\??\'+$old+'\retype_ime.dll'))) 'Exact observed Inno Windows marker on old DLL'
foreach ($directory in @($root,($root+'\versions'),$old,($old+'\x86'),($old+'\dictionaries'),($old+'\settings'))) {
  Assert-True (Policy @(Entry $directory) -Directories @($directory)) "Verified empty-at-reboot directory operation $directory"
  Assert-True (Policy @(Entry ('*1\??\'+$directory)) -Directories @($directory)) "Observed marker on verified directory $directory"
}
Assert-Denied 'Unknown marker is not generalized' { Policy @(Entry ('*2\??\'+$old+'\retype_ime.dll')) }
foreach ($marker in @('*0','*01','*11','*999','*1!','*')) {
  $script:BadMarker=$marker
  Assert-Denied "Unknown source marker $marker" { Policy @(Entry ($script:BadMarker+'\??\'+$root)) -Directories @($root) }
}
Assert-Denied 'Observed marker requires exact NT prefix' { Policy @(Entry ('*1'+$root)) -Directories @($root) }
Assert-Denied 'Observed marker does not admit extended-prefix variant' { Policy @(Entry ('*1\\?\'+$root)) -Directories @($root) }
Assert-Denied 'Known directory destination must remain empty' { Policy @(Entry $root 'C:\Temp\moved') -Directories @($root) }
Assert-Denied 'Known directory rename within product rejected' { Policy @(Entry $old ($root+'\versions')) -Directories @($old) }
Assert-Denied 'Known directory whitespace destination rejected' { Policy @(Entry $old ' ') -Directories @($old) }
Assert-Denied 'Missing or file-typed versions parent rejected' { Policy @(Entry ($root+'\versions')) }
Assert-Denied 'Directory-typed DLL filename rejected' { Policy @(Entry ($old+'\retype_ime.dll')) -Directories @($old+'\retype_ime.dll') }
foreach ($directory in @($new,($new+'\x86'),($new+'\dictionaries'),($new+'\settings'),($new+'\other\nested'))) {
  $script:BadDirectory=$directory
  Assert-Denied "New payload or child directory $directory" { Policy @(Entry ('*1\??\'+$script:BadDirectory)) -Directories @($script:BadDirectory) }
}
foreach ($directory in @(($old+'\cache'),($old+'\x86\nested'),($root+'\other'),($root+'\versions\invalid'),($old+'\..'),($old+'\'),($old+':stream'))) {
  $script:BadDirectory=$directory
  Assert-Denied "Directory outside narrow canonical whitelist $directory" { Policy @(Entry ('*1\??\'+$script:BadDirectory)) -Directories @($script:BadDirectory) }
}
Assert-Denied 'Verified directory cannot traverse a reparse point' { Policy @(Entry ('*1\??\'+$old+'\x86')) @($old) -Directories @($old+'\x86') }

# Reproduce the real eight deletion pairs from the Inno Setup 6.7.3 uninstall
# snapshot work/research/lifecycle-final/pending-own-deletes.json, without any
# native registry/filesystem adapter. Directory attributes are explicit fixtures.
$actualVersions=@('0.2.0-20261109023323-821500','0.1.0-20261107194906-101470','0.1.0-20261107191702-788314')
$actualDirs=@($actualVersions | ForEach-Object { $root+'\versions\'+$_ }) + @(($root+'\versions'),$root)
$actualPairs=@($actualVersions | ForEach-Object { Entry ('*1\??\'+$root+'\versions\'+$_+'\retype_ime.dll') }) + @($actualDirs | ForEach-Object { Entry ('*1\??\'+$_) })
Assert-True ($actualPairs.Count -eq 8) 'Real snapshot pair count'
Assert-True (Policy $actualPairs -Directories $actualDirs) 'Real eight deletion pairs allow fresh unique payload'
Assert-Denied 'Real snapshot directories still require native type proof' { Policy $actualPairs }
foreach ($path in @($root,($root+'\retype_ime.dll'),($root+'\unins000.exe'),($old+'\retype-learning-host.exe'),($old+'\user-profile.ps1'),$old,($old+'\other.dll'),($old+'\x86\other.dll'),($old+'\x64\retype_ime.dll'))) {
  $script:BadPath=$path
  Assert-Denied "Unsafe deletion $path" { Policy @(Entry $script:BadPath) }
}
Assert-Denied 'Rename old DLL outside product' { Policy @(Entry ($old+'\retype_ime.dll') 'C:\Temp\old.dll') }
Assert-Denied 'Rename old DLL to itself' { Policy @(Entry ($old+'\retype_ime.dll') ($old+'\retype_ime.dll')) }
Assert-Denied 'Inbound rename to product' { Policy @(Entry 'C:\Temp\replacement.dll' ($old+'\retype_ime.dll')) }
Assert-Denied 'Replacement destination marker' { Policy @(Entry 'C:\Temp\replacement.dll' ('!\??\'+$new+'\retype_ime.dll')) }
Assert-Denied 'Nonempty whitespace destination' { Policy @(Entry ($old+'\retype_ime.dll') ' ') }
Assert-Denied 'Delete new x64 payload' { Policy @(Entry ($new+'\retype_ime.dll')) }
Assert-Denied 'Delete new x86 payload' { Policy @(Entry ($new+'\x86\retype_ime.dll')) }
Assert-Denied 'Traversal out of version' { Policy @(Entry ($old+'\..\..\user-profile.ps1')) }
Assert-Denied 'Traversal into another version' { Policy @(Entry ($old+'\..\0.1.0-20261107220000-222222\retype_ime.dll')) }
Assert-Denied 'Traversal from outside into product' { Policy @(Entry ('C:\Program Files\other\..\EnglishAssist\versions\0.1.0-20261107191702-788314\retype_ime.dll')) }
Assert-Denied 'Forward slash alias' { Policy @(Entry (($old+'\retype_ime.dll').Replace('\','/'))) }
Assert-Denied 'Unknown NT device alias' { Policy @(Entry '\Device\HarddiskVolume3\Program Files\EnglishAssist\versions\0.1.0-20261107191702-788314\retype_ime.dll') }
foreach ($version in @('old','0.1.0-20260230000000-123','0.1.0-20261107249999-1','0.1.0-20261107191702-1234567','99999.1.0-20261107191702-1','0.1-20261107191702-1')) {
  $script:BadVersion=$version
  Assert-Denied "Invalid version $version" { Policy @(Entry ($root+'\versions\'+$script:BadVersion+'\retype_ime.dll')) }
}
foreach ($link in @($root,($root+'\versions'),$old,($old+'\x86'),($old+'\x86\retype_ime.dll'))) {
  $script:BadLink=$link
  Assert-Denied "Reparse ancestor $link" { Policy @(Entry ($old+'\x86\retype_ime.dll')) @($script:BadLink) }
}
Assert-Denied 'Reparse new payload even without pending files' { Policy @() @($new) }
Assert-Denied 'Incomplete pair' { Policy @([pscustomobject]@{Source=($old+'\retype_ime.dll')}) }
Assert-Denied 'Unvalidated new payload suffix' { Policy @() @() ($root+'\versions\new') }
Assert-Denied 'Existing new payload must never be reused' {
  Test-EnglishAssistPendingRenamePolicy -InstallRoot $root -NewPayloadDirectory $new -NewPayloadExists $true -ProgramFilesDirectory 'C:\Program Files'
}
Assert-Denied 'Known DOS short source alias cannot bypass strict spelling' {
  Test-EnglishAssistPendingRenamePolicy -InstallRoot $root -NewPayloadDirectory $new -Entries @(Entry ('C:\PROGRA~1\ENGLIS~1\versions\'+[IO.Path]::GetFileName($old)+'\retype_ime.dll')) -ProgramFilesDirectory 'C:\Program Files' -RootAliases @('C:\PROGRA~1\ENGLIS~1')
}
Assert-Denied 'Known DOS short destination alias cannot target new payload' {
  Test-EnglishAssistPendingRenamePolicy -InstallRoot $root -NewPayloadDirectory $new -Entries @(Entry 'C:\Temp\a' ('C:\PROGRA~1\ENGLIS~1\versions\'+[IO.Path]::GetFileName($new)+'\retype_ime.dll')) -ProgramFilesDirectory 'C:\Program Files' -RootAliases @('C:\PROGRA~1\ENGLIS~1')
}

$nul=[string][char]0
$raw=('\??\'+$old+'\retype_ime.dll')+$nul+$nul+('C:\Temp\a')+$nul+('C:\Temp\b')+$nul+$nul
$entries=@(ConvertFrom-EnglishAssistPendingRenameValue $raw)
Assert-True ($entries.Count -eq 2 -and $entries[0].Destination -eq '' -and $entries[1].Destination -eq 'C:\Temp\b') 'Decode preserves deletion and rename pairs'
Assert-True (Policy $entries) 'Decoded queue policy'
$entries=@(ConvertFrom-EnglishAssistPendingRenameValue (($old+'\retype_ime.dll')+$nul+$nul))
Assert-True ($entries.Count -eq 1 -and $entries[0].Destination -eq '') 'Deletion with two trailing NULs'
$entries=@(ConvertFrom-EnglishAssistPendingRenameValue (($old+'\retype_ime.dll')+$nul+$nul+$nul))
Assert-True ($entries.Count -eq 1 -and $entries[0].Destination -eq '') 'Deletion with explicit final MULTI_SZ NUL'
Assert-True (@(ConvertFrom-EnglishAssistPendingRenameValue '').Count -eq 0) 'Absent registry value'
Assert-True (@(ConvertFrom-EnglishAssistPendingRenameValue ($nul+$nul)).Count -eq 0) 'Empty MULTI_SZ value'
Assert-Denied 'Unterminated MULTI_SZ' { ConvertFrom-EnglishAssistPendingRenameValue ($old+'\retype_ime.dll') }
Assert-Denied 'Odd source without destination' { ConvertFrom-EnglishAssistPendingRenameValue (($old+'\retype_ime.dll')+$nul) }
Assert-Denied 'Interior empty source before more entries' { ConvertFrom-EnglishAssistPendingRenameValue ($nul+'C:\Temp\a'+$nul+$nul) }
Assert-Denied 'Decoded rename to new payload' {
  $parsed=@(ConvertFrom-EnglishAssistPendingRenameValue ('C:\Temp\a'+$nul+($new+'\retype_ime.dll')+$nul+$nul))
  Policy $parsed
}
Write-Output "$script:Passed pending-rename policy/parser cases passed; no registry, files or processes changed."

