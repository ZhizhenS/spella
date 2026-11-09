# Pure helpers only: never execute the cleanup body, query real processes,
# touch registry/COM, or terminate any process.
$ErrorActionPreference = 'Stop'
$sourcePath = Join-Path $PSScriptRoot '..\cleanup-english-assist.ps1'
$tokens = $null; $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($sourcePath, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'Cleanup source does not parse.' }
$names = @('Write-EnglishAssistCleanupDiagnostic','Set-EnglishAssistCleanupPhase','Test-EnglishAssistSameProcess','Stop-EnglishAssistOwnedHelper','Remove-EnglishAssistUserTsfState')
$definitions = @($ast.FindAll({ param($node)
  $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $names
}, $true))
if ($definitions.Count -ne $names.Count) { throw 'Cleanup helper definitions were not found.' }
. ([scriptblock]::Create(($definitions | ForEach-Object { $_.Extent.Text }) -join [Environment]::NewLine))
$script:Passed = 0
function Assert-True([bool]$Condition,[string]$Name) {
  if (-not $Condition) { throw $Name }
  $script:Passed++
}
function Assert-Fails([scriptblock]$Action,[string]$Name) {
  $failed=$false
  try { $null=& $Action } catch { $failed=$true }
  Assert-True $failed $Name
}
$snapshot=[pscustomobject]@{ProcessId=321;CreationDate=[datetime]'2026-01-01T00:00:00Z';Name='retype-learning-host.exe';ExecutablePath='C:\fixture\EnglishAssist\versions\v1\retype-learning-host.exe'}
$reused=[pscustomobject]@{ProcessId=321;CreationDate=[datetime]'2026-01-01T00:00:01Z';Name=$snapshot.Name;ExecutablePath=$snapshot.ExecutablePath}
$otherImage=[pscustomobject]@{ProcessId=321;CreationDate=$snapshot.CreationDate;Name='other.exe';ExecutablePath='C:\fixture\other.exe'}
$owner={param($Process) [pscustomobject]@{ReturnValue=0;Sid='fixture-owner'}}
$stop={param($Process) $script:StopCount++;'Stopped'}
$script:StopCount=0
Assert-True ((Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' {param($Id) $null} $owner $stop) -eq 'Gone') 'Already exited process was not accepted'
Assert-True ((Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' {param($Id) $reused} $owner $stop) -eq 'Gone') 'Reused PID was not recognized'
Assert-True ($script:StopCount -eq 0) 'Absent/reused PID was terminated'
Assert-True ((Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' {param($Id) $snapshot} $owner $stop) -eq 'Stopped') 'Verified helper did not stop'
Assert-True ($script:StopCount -eq 1) 'Verified helper stop count is wrong'
Assert-True ((Stop-EnglishAssistOwnedHelper $snapshot 'another-owner' {param($Id) $snapshot} $owner $stop) -eq 'OtherOwner') 'Unrelated owner was not preserved'
Assert-True ($script:StopCount -eq 1) 'Another user helper was terminated'

$script:LookupCount=0
$exitAfterLookup={param($Id) $script:LookupCount++;if($script:LookupCount -eq 1){$snapshot}else{$null}}
Assert-True ((Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' $exitAfterLookup {throw 'Owner lookup raced with exit'} $stop) -eq 'Gone') 'Exit during owner lookup still failed'
$script:LookupCount=0
Assert-True ((Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' $exitAfterLookup $owner {throw 'Kill raced with exit'}) -eq 'Gone') 'Exit during termination still failed'
$script:LookupCount=0
$reuseAfterLookup={param($Id) $script:LookupCount++;if($script:LookupCount -eq 1){$snapshot}else{$reused}}
Assert-True ((Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' $reuseAfterLookup $owner {throw 'Original process exited'}) -eq 'Gone') 'PID reuse after failed termination was not accepted as original exit'
Assert-Fails { Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' {param($Id) $snapshot} $owner {throw [UnauthorizedAccessException]::new('Denied')} } 'Live helper termination failure was swallowed'
Assert-Fails { Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' {throw 'Cannot establish process identity'} $owner $stop } 'Unknown process state was treated as gone'
Assert-Fails { Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' {param($Id) $otherImage} $owner $stop } 'Same-time changed executable was accepted'
Assert-Fails { Stop-EnglishAssistOwnedHelper $snapshot 'fixture-owner' {param($Id) $snapshot} {param($Process) [pscustomobject]@{ReturnValue=2;Sid=$null}} $stop } 'Unknown owner was accepted'
Assert-True ($script:StopCount -eq 1) 'A refused helper was terminated'

# All registry operations below are injected callbacks. The production target
# remains a non-parameterized provider-qualified constant, never a shared root.
$expectedTipPath='Registry::HKEY_CURRENT_USER\Software\Microsoft\CTF\TIP\{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}'
$script:DeleteCount=0
$script:TsfPaths=@()
$missing={param($Path) $script:TsfPaths+=,$Path;$false}
$delete={param($Path) $script:TsfPaths+=,$Path;$script:DeleteCount++;$script:TombstoneExists=$false}
Remove-EnglishAssistUserTsfState $missing $delete
Assert-True ($script:DeleteCount -eq 0) 'Missing own user TSF key was deleted'
$script:TombstoneExists=$true
$present={param($Path) $script:TsfPaths+=,$Path;$script:TombstoneExists}
Remove-EnglishAssistUserTsfState $present $delete
Assert-True ($script:DeleteCount -eq 1 -and -not $script:TombstoneExists) 'Own TSF tombstone was not removed and verified'
Assert-True (@($script:TsfPaths | Where-Object { $_ -cne $expectedTipPath }).Count -eq 0) 'TSF cleanup accessed another IME or shared registry tree'
Assert-Fails { Remove-EnglishAssistUserTsfState {param($Path) $true} {throw 'Fixture access denied'} } 'Own TSF delete failure was swallowed'
Assert-Fails { Remove-EnglishAssistUserTsfState {param($Path) $true} {param($Path)} } 'Still-present TSF tombstone was treated as removed'
Assert-Fails { Remove-EnglishAssistUserTsfState {throw 'Fixture read failure'} {throw 'Must not delete'} } 'Unknown TSF state was treated as missing'

$fixtureFile=[IO.Path]::Combine([IO.Path]::GetTempPath(),('english-assist-cleanup-test-'+[Guid]::NewGuid().ToString('N')+'.jsonl'))
try {
  $script:CleanupLogPath=$fixtureFile
  $script:CleanupPhase='FixtureFailure'
  $secret='fixture-key-and-input-must-not-appear'
  try { throw [InvalidOperationException]::new($secret,[Runtime.InteropServices.COMException]::new($secret,-2147024891)) }
  catch { Write-EnglishAssistCleanupDiagnostic 'Failure' $_ 321 }
  $raw=[IO.File]::ReadAllText($fixtureFile)
  $record=$raw | ConvertFrom-Json
  Assert-True (-not $raw.Contains($secret)) 'Cleanup diagnostic leaked exception text'
  Assert-True ($record.Phase -eq 'FixtureFailure' -and $record.ProcessId -eq 321 -and $record.Event -eq 'Failure') 'Cleanup diagnostic lacks phase/process metadata'
  Assert-True ($record.Exceptions.Count -eq 2 -and $record.Exceptions[1].HResult -eq -2147024891) 'Cleanup diagnostic lost HRESULT chain'
} finally {
  if ([IO.File]::Exists($fixtureFile)) { [IO.File]::Delete($fixtureFile) }
}
$source=[IO.File]::ReadAllText($sourcePath)
Assert-True ($source.IndexOf("Set-EnglishAssistCleanupPhase 'BlockHelperRestart'") -gt $source.IndexOf('$taskMode = Get-EnglishAssistCleanupMode')) 'Uninstall marker precedes ownership validation'
Assert-True ($source.IndexOf("Set-EnglishAssistCleanupPhase 'RemoveOwnerMetadata'") -gt $source.IndexOf("Set-EnglishAssistCleanupPhase 'RemoveRequestedProductData'")) 'Owner metadata is removed before data cleanup'
Assert-True ($source.Contains("[IO.Path]::Combine([IO.Path]::GetTempPath(),'EnglishAssist-cleanup-logs')")) 'Cleanup log could recreate removed product data'
Assert-True ($source.IndexOf('Initialize-EnglishAssistPowerShell') -lt $source.IndexOf("Write-EnglishAssistCleanupDiagnostic 'Start'")) 'Cleanup diagnostics autoload before native PowerShell bootstrap'
Assert-True ($source.IndexOf("Set-EnglishAssistCleanupPhase 'RemoveOwnUserTsfState'") -gt $source.IndexOf("if (-not `$?) { throw 'English keyboard removal failed.' }")) 'Own user TSF state is removed before successful keyboard removal'
Assert-True ($source.IndexOf("Set-EnglishAssistCleanupPhase 'RemoveOwnUserTsfState'") -lt $source.IndexOf("Set-EnglishAssistCleanupPhase 'RemoveOwnerMetadata'")) 'Own user TSF cleanup error could discard ownership metadata'
Write-Output "$script:Passed cleanup identity/race/diagnostic/own-TIP tombstone cases passed; no real processes, registry, credentials or product data touched."
