# Isolated file fixtures only. Never read native preferences/ProfileList, initialize
# a real user profile, invoke COM, change ACLs, or schedule deletion.
$ErrorActionPreference='Stop'
$source=Join-Path $PSScriptRoot '..\install-state.ps1'
$tokens=$null;$errors=$null
$ast=[Management.Automation.Language.Parser]::ParseFile($source,[ref]$tokens,[ref]$errors)
if ($errors.Count) { throw 'Install state source does not parse.' }
$names=@('Get-EnglishAssistInputPreferenceNames','Resolve-EnglishAssistInputProfile',
 'Get-EnglishAssistInputPreferenceAttributes','Get-EnglishAssistInputPreferenceLayout',
 'Assert-EnglishAssistInputPreferenceImage','Read-EnglishAssistInputPreferenceImage',
 'Initialize-EnglishAssistInputPreferences','Remove-EnglishAssistInputPreferences')
$definitions=@($ast.FindAll({param($node) $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $names},$true))
if ($definitions.Count -ne $names.Count) { throw 'Shared preferences helpers are incomplete.' }
. ([scriptblock]::Create(($definitions | ForEach-Object {$_.Extent.Text}) -join [Environment]::NewLine))
$script:Passed=0
function Assert-True([bool]$Condition,[string]$Name) { if (-not $Condition) { throw $Name };$script:Passed++ }
function Assert-Rejected([scriptblock]$Action,[string]$Name) {
 $failed=$false;try {$null=& $Action} catch {$failed=$true};Assert-True $failed $Name
}
# Slot order is a persisted cross-language ABI. Reordering NAMES requires a
# new schema version; equal counts or sets alone cannot establish compatibility.
$rustSource=[IO.File]::ReadAllText((Join-Path $PSScriptRoot '..\..\common\user_preferences.rs'))
$rustTable=[regex]::Match($rustSource,'(?s)pub\s+const\s+NAMES\s*:\s*&\s*\[\s*&str\s*\]\s*=\s*&\s*\[(.*?)\]\s*;')
$rustNames=@([regex]::Matches($rustTable.Groups[1].Value,'"([A-Za-z][A-Za-z0-9_]*)"') | ForEach-Object {$_.Groups[1].Value})
$psNames=@(Get-EnglishAssistInputPreferenceNames)
Assert-True ($rustTable.Success -and $rustNames.Count -eq $psNames.Count -and ($rustNames -join [char]0) -ceq ($psNames -join [char]0)) 'Rust/PowerShell names match exactly in case and every slot index'
Assert-True ((@(Get-EnglishAssistInputPreferenceNames)).Count -eq 23) 'Known migration fields'
Assert-True ((@(Get-EnglishAssistInputPreferenceNames))[6] -ceq 'EnglishAutoSpaceDisabled') 'Spacing slot index'
$sid='S-1-5-21-111-222-333-1001'
Assert-True ((Resolve-EnglishAssistInputProfile $sid {param($Id) if ($Id -ne $sid) {throw 'Wrong SID'};'C:\Users\Fixture'}) -eq 'C:\Users\Fixture') 'Resolve only selected owner profile'
Assert-Rejected {Resolve-EnglishAssistInputProfile 'other' {throw 'Must not query'}} 'Bad SID before registry adapter'
Assert-Rejected {Resolve-EnglishAssistInputProfile $sid {$null}} 'Missing profile rejected'
$script:Attributes=@{}
$fakeAttributes={param($Path) if ($script:Attributes.ContainsKey($Path)) {$script:Attributes[$Path]} elseif ($Path -eq 'C:\Users\Fixture\.EnglishAssist' -or $Path -eq 'C:\Users\Fixture\.EnglishAssist\input-preferences-v1.bin') {-1} else {0x10}}
$layout=Get-EnglishAssistInputPreferenceLayout 'C:\Users\Fixture' $fakeAttributes
Assert-True ($layout.File -ceq 'C:\Users\Fixture\.EnglishAssist\input-preferences-v1.bin') 'Fixed file beneath profile, outside AppData'
foreach ($bad in @('relative','C:\','\\host\share\profile','C:\Users\Fixture\..\Other','C:\Users\Fixture:stream')) {
 Assert-Rejected {Get-EnglishAssistInputPreferenceLayout $bad $fakeAttributes} ('Reject path '+$bad)
}
foreach ($path in @('C:\','C:\Users','C:\Users\Fixture',$layout.Directory,$layout.File)) {
 $script:Attributes=@{};$script:Attributes[$path]=0x410
 Assert-Rejected {Get-EnglishAssistInputPreferenceLayout 'C:\Users\Fixture' $fakeAttributes} 'Reject reparse chain'
}
$script:Attributes=@{};$script:Attributes[$layout.Directory]=0x80
Assert-Rejected {Get-EnglishAssistInputPreferenceLayout 'C:\Users\Fixture' $fakeAttributes} 'Reject file as directory'
$script:Attributes=@{};$script:Attributes[$layout.File]=0x10
Assert-Rejected {Get-EnglishAssistInputPreferenceLayout 'C:\Users\Fixture' $fakeAttributes} 'Reject directory as store'

$fixture=[IO.Path]::Combine([IO.Path]::GetTempPath(),('EnglishAssist-input-prefs-fixture-'+[Guid]::NewGuid().ToString('N')))
$null=[IO.Directory]::CreateDirectory($fixture)
try {
 $script:Legacy=@{EnglishAutoSpaceDisabled=[uint32]0;StatusBarX=[uint32]::MaxValue;CandidateTheme=[uint32]2}
 $script:Reads=@()
 $read={param($Name) $script:Reads+=$Name;if ($script:Legacy.ContainsKey($Name)) {$script:Legacy[$Name]} else {$null}}
 $file=Initialize-EnglishAssistInputPreferences $fixture $read
 $bytes=Read-EnglishAssistInputPreferenceImage $file
 Assert-True ($bytes.Length -eq 288) 'Exact fixed length'
 Assert-True ([Text.Encoding]::ASCII.GetString($bytes,0,8) -ceq 'EAINPF01') 'File magic'
 Assert-True ([BitConverter]::ToUInt32($bytes,80) -eq 1 -and [BitConverter]::ToUInt32($bytes,84) -eq 0) 'Some zero preserved'
 Assert-True ([BitConverter]::ToUInt32($bytes,32) -eq 0 -and [BitConverter]::ToUInt32($bytes,36) -eq 0) 'None remains distinct'
 Assert-True ([BitConverter]::ToUInt32($bytes,100) -eq [uint32]::MaxValue) 'All coordinate DWORD bits preserved'
 Assert-True ($script:Reads.Count -eq 23 -and (@($script:Reads | Where-Object {$_ -notin @(Get-EnglishAssistInputPreferenceNames)})).Count -eq 0) 'Migration never reads unrelated values'
 $original=[Convert]::ToBase64String($bytes)
 $hold=[IO.File]::Open($file,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
 try {
  $again=Initialize-EnglishAssistInputPreferences $fixture {throw 'Existing file must not reread legacy registry'}
  Assert-True ($again -eq $file -and [Convert]::ToBase64String([IO.File]::ReadAllBytes($file)) -ceq $original) 'Valid in-use file preserved byte-for-byte'
 } finally {$hold.Dispose()}
 foreach ($offset in @(0,8,12,16,31,216,287)) {
  $bad=[byte[]]$bytes.Clone();$bad[$offset]=0x7f
  Assert-Rejected {Assert-EnglishAssistInputPreferenceImage $bad} 'Reject malformed header/reserved slot'
 }
 $bad=[byte[]]$bytes.Clone();$bad[32]=2
 Assert-Rejected {Assert-EnglishAssistInputPreferenceImage $bad} 'Reject invalid have marker'
 $bad=[byte[]]$bytes.Clone();$bad[36]=1
 Assert-EnglishAssistInputPreferenceImage $bad
 Assert-True ([BitConverter]::ToUInt32($bad,32) -eq 0) 'Unpublished value ignored during concurrent atomic publish'
 Assert-Rejected {Assert-EnglishAssistInputPreferenceImage (New-Object byte[] 287)} 'Reject short image'
 Assert-Rejected {Assert-EnglishAssistInputPreferenceImage (New-Object byte[] 289)} 'Reject extended image'
 $bad=[byte[]]$bytes.Clone();$bad[32]=2
 [IO.File]::WriteAllBytes($file,$bad)
 Assert-Rejected {Initialize-EnglishAssistInputPreferences $fixture $read} 'Corrupt existing store is never replaced'
 Assert-Rejected {Remove-EnglishAssistInputPreferences $fixture} 'Corrupt store is never deleted'
 Assert-True ([Convert]::ToBase64String([IO.File]::ReadAllBytes($file)) -ceq [Convert]::ToBase64String($bad)) 'Bad file remains intact'
 [IO.File]::WriteAllBytes($file,$bytes)
 $hold=[IO.File]::Open($file,[IO.FileMode]::Open,[IO.FileAccess]::Read,[IO.FileShare]::Read)
 try {
  Assert-Rejected {Remove-EnglishAssistInputPreferences $fixture} 'Busy deletion explicitly fails'
  Assert-True ([IO.File]::Exists($file)) 'Busy fixed file preserved, not queued or renamed'
 } finally {$hold.Dispose()}
 $other=[IO.Path]::Combine([IO.Path]::GetDirectoryName($file),'unrelated.txt')
 [IO.File]::WriteAllText($other,'fixture')
 Remove-EnglishAssistInputPreferences $fixture
 Assert-True (-not [IO.File]::Exists($file) -and [IO.File]::Exists($other)) 'Only exact owned file deleted'
 Assert-True ([IO.Directory]::Exists([IO.Path]::GetDirectoryName($file))) 'Nonempty directory preserved'
 [IO.File]::Delete($other)
 Remove-EnglishAssistInputPreferences $fixture
 Assert-True (-not [IO.Directory]::Exists([IO.Path]::GetDirectoryName($file))) 'Only empty owned directory removed'
 $failedProfile=[IO.Path]::Combine($fixture,'failed-profile');$null=[IO.Directory]::CreateDirectory($failedProfile)
 Assert-Rejected {Initialize-EnglishAssistInputPreferences $failedProfile {throw 'Invalid legacy type'}} 'Migration failure before store creation'
 Assert-True (-not [IO.Directory]::Exists([IO.Path]::Combine($failedProfile,'.EnglishAssist'))) 'Migration failure leaves no partial store'
} finally {
 # This is the unique temporary fixture created above, never a resolved profile.
 if ([IO.Path]::GetDirectoryName($fixture) -ne [IO.Path]::GetTempPath().TrimEnd('\') -or [IO.Path]::GetFileName($fixture) -notlike 'EnglishAssist-input-prefs-fixture-*') {throw 'Unsafe fixture cleanup'}
 [IO.Directory]::Delete($fixture,$true)
}
Write-Output "$script:Passed shared input preferences format/path/migration/delete cases passed; temporary fixtures only, no real profile, registry, ACL, COM or reboot queue changes."
