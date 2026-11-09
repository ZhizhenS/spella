# Pure installer diagnostics/retry tests. The enrollment body is never executed.
$ErrorActionPreference = 'Stop'
$sourcePath = Join-Path $PSScriptRoot '..\user-profile.ps1'
$tokens = $null; $parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($sourcePath, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'The user-profile script does not parse.' }
$names = @('Write-EnglishAssistEnrollmentDiagnostic', 'Set-EnglishAssistEnrollmentPhase', 'Wait-EnglishAssistProfileState')
$definitions = @($ast.FindAll({ param($node)
  $node -is [Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -in $names
}, $true))
if ($definitions.Count -ne $names.Count) { throw 'Enrollment helper definitions were not found.' }
$helperCode = ($definitions | ForEach-Object { $_.Extent.Text }) -join [Environment]::NewLine
. ([scriptblock]::Create($helperCode))
$script:Passed = 0
function Assert-True([bool]$Condition, [string]$Name) {
  if (-not $Condition) { throw $Name }
  $script:Passed++
}

$source = [IO.File]::ReadAllText($sourcePath)
Assert-True (-not $source.Contains("'LearningAcl'") -and -not $source.Contains("'StartLearningHost'")) 'Enrollment still initializes or starts automatic learning'
Assert-True (-not $source.Contains("'--serve'")) 'Enrollment still launches a persistent learning server'
$launchCommands = @($ast.FindAll({ param($node)
  $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Start-Process'
}, $true))
foreach ($command in $launchCommands) {
  Assert-True ($command.Extent.Text.Contains("'--stop'")) 'Enrollment starts a process beyond the explicit uninstall stop command'
}
$writes = @($ast.FindAll({ param($node)
  $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -in @('New-Item','New-ItemProperty','Set-ItemProperty')
}, $true))
foreach ($command in $writes) {
  Assert-True (-not ($command.Extent.Text -match 'EnglishAssist Learning|learningRunPath|CurrentVersion\\Run')) 'Enrollment writes a learning autorun entry'
}
Assert-True ($source.IndexOf("Set-EnglishAssistEnrollmentPhase 'CompleteEnrollment'") -gt $source.IndexOf("Set-EnglishAssistEnrollmentPhase 'VerifyPreservedKeyboards'")) 'Enrollment clears helper block before final verification'
Assert-True ($source.Contains("-Name 'Uninstalling' -PropertyType DWord -Value 0")) 'Enrollment does not publish an explicit allow marker'
Assert-True ($source.IndexOf('::DeactivateOwnProfile($entry.Language, $entry.Profile)') -lt $source.IndexOf("Set-EnglishAssistEnrollmentPhase 'InstallLayoutOrTip'")) 'Own profile is removed before deactivation'

$fixtureDirectory = Join-Path ([IO.Path]::GetTempPath()) ('english-assist-profile-test-' + [Guid]::NewGuid().ToString('N'))
$null = [IO.Directory]::CreateDirectory($fixtureDirectory)
$script:EnrollmentPhase = 'Fixture'
$script:EnrollmentLogPath = Join-Path $fixtureDirectory 'diagnostics.jsonl'
$script:EnrollmentStarted = [Diagnostics.Stopwatch]::StartNew()
$script:EnrollmentElevated = $false
$script:EnrollmentSessionId = 0
$Uninstall = $false; $Verify = $false; $InspectOnly = $false
$fixtureScript = Join-Path $fixtureDirectory 'trap.ps1'
$trapLog = Join-Path $fixtureDirectory 'trap.jsonl'
try {
  $secret = 'fixture-secret-must-not-appear'
  try { throw [InvalidOperationException]::new($secret, [Runtime.InteropServices.COMException]::new($secret, -2147467259)) }
  catch { Write-EnglishAssistEnrollmentDiagnostic 'Failure' $_ }
  $raw = [IO.File]::ReadAllText($script:EnrollmentLogPath)
  $record = $raw | ConvertFrom-Json
  Assert-True (-not $raw.Contains($secret)) 'Diagnostic leaked an exception message'
  Assert-True ($record.Phase -eq 'Fixture' -and $record.Event -eq 'Failure') 'Structured phase/event missing'
  Assert-True ($record.ProcessBits -eq ([IntPtr]::Size * 8)) 'Process bitness missing'
  Assert-True ($record.Exceptions.Count -eq 2 -and $record.Exceptions[1].HResult -eq -2147467259) 'Nested HRESULT lost'
  Assert-True ($record.ScriptLine -gt 0) 'Failure line missing'
  try { throw [Runtime.InteropServices.COMException]::new($secret, 1) }
  catch { Write-EnglishAssistEnrollmentDiagnostic 'Failure' $_ }
  $lastRecord = Get-Content -LiteralPath $script:EnrollmentLogPath | Select-Object -Last 1 | ConvertFrom-Json
  Assert-True ($lastRecord.Exceptions[0].HResult -eq 1) 'S_FALSE diagnostic lost its actual HRESULT'

  $script:readCount = 0
  $result = Wait-EnglishAssistProfileState { $script:readCount++; $script:readCount } { param($state) $state -eq 3 } 'Not ready' 4 0
  Assert-True ($result -eq 3 -and $script:readCount -eq 3) 'False snapshots did not converge exactly'
  $script:readCount = 0
  $result = Wait-EnglishAssistProfileState {
    $script:readCount++
    if ($script:readCount -lt 3) { throw [Runtime.InteropServices.COMException]::new('Transient', -2147467259) }
    'ready'
  } { param($state) $state -eq 'ready' } 'Not ready' 4 0
  Assert-True ($result -eq 'ready' -and $script:readCount -eq 3) 'Read exceptions did not converge'
  $script:readCount = 0; $failed = $false
  try { Wait-EnglishAssistProfileState { $script:readCount++; $false } { param($state) $state } 'Still disabled' 3 0 }
  catch { $failed = $_.Exception.Message -eq 'Still disabled' }
  Assert-True ($failed -and $script:readCount -eq 3) 'Exhausted false state was swallowed'
  $script:readCount = 0; $failed = $false
  try { Wait-EnglishAssistProfileState { $script:readCount++; throw [Runtime.InteropServices.COMException]::new('Read failed', -2147024891) } { $true } 'unused' 2 0 }
  catch { $failed = $_.Exception -is [Runtime.InteropServices.COMException] -and $_.Exception.HResult -eq -2147024891 }
  Assert-True ($failed -and $script:readCount -eq 2) 'Final read exception was swallowed or replaced'
  $script:readCount = 0; $failed = $false
  try { Wait-EnglishAssistProfileState { $script:readCount++; $true } { throw 'Invalid predicate' } 'unused' 3 0 }
  catch { $failed = $_.Exception.Message -eq 'Invalid predicate' }
  Assert-True ($failed -and $script:readCount -eq 1) 'Invalid readiness logic was retried'

  # Invoke the actual top-level trap in a child of the same PowerShell bitness.
  $trap = @($ast.FindAll({ param($node) $node -is [Management.Automation.Language.TrapStatementAst] }, $true))
  if ($trap.Count -ne 1) { throw 'Expected one top-level trap.' }
  $child = @(
    '$ErrorActionPreference = ''Stop'''
    '$script:EnrollmentPhase = ''FixtureTrap'''
    ('$script:EnrollmentLogPath = ''' + $trapLog.Replace("'", "''") + "'")
    '$script:EnrollmentStarted = [Diagnostics.Stopwatch]::StartNew()'
    $helperCode
    $trap[0].Extent.Text
    ('throw ''' + $secret + "'")
    '''UNREACHABLE'''
  ) -join [Environment]::NewLine
  [IO.File]::WriteAllText($fixtureScript, $child, [Text.UTF8Encoding]::new($false))
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = [Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
  $start.Arguments = '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + $fixtureScript + '"'
  $start.UseShellExecute = $false; $start.CreateNoWindow = $true
  $start.RedirectStandardOutput = $true; $start.RedirectStandardError = $true
  $process = [Diagnostics.Process]::Start($start)
  $stdout = $process.StandardOutput.ReadToEnd(); $stderr = $process.StandardError.ReadToEnd()
  $process.WaitForExit()
  $trapRecord = [IO.File]::ReadAllText($trapLog) | ConvertFrom-Json
  Assert-True ($process.ExitCode -eq 1) 'Top-level trap did not return failure'
  Assert-True ($trapRecord.Phase -eq 'FixtureTrap' -and $trapRecord.Event -eq 'Failure') 'Trap did not persist its diagnostic'
  Assert-True (-not ($stdout + $stderr + [IO.File]::ReadAllText($trapLog)).Contains($secret)) 'Trap leaked exception text'
  Assert-True (-not $stdout.Contains('UNREACHABLE')) 'Script continued after trapped failure'
  $process.Dispose()
} finally {
  # No recursive cleanup or path computed from enrollment state.
  foreach ($path in @($script:EnrollmentLogPath, $fixtureScript, $trapLog)) {
    if (Test-Path -LiteralPath $path) { Remove-Item -LiteralPath $path -Force }
  }
  Remove-Item -LiteralPath $fixtureDirectory -Force
}
Write-Output "$script:Passed enrollment diagnostic/retry cases passed; no keyboard, registry, ACL or host changes."
