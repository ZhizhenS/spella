# English Assist user cleanup. Run as the user whose English keyboard is being removed.
[CmdletBinding()]
param(
  [Parameter(Mandatory=$true)][string]$InstallRoot,
  [switch]$DeletePersonalData
)
$ErrorActionPreference = 'Stop'
$script:CleanupPhase = 'Initialize'
$script:CleanupLogPath = $null
function Write-EnglishAssistCleanupDiagnostic([string]$Event, $Failure=$null, [int]$ProcessId=0) {
  try {
    if (-not $script:CleanupLogPath) { return }
    $chain=@();$exception=$(if($Failure){$Failure.Exception}else{$null})
    while ($exception -and $chain.Count -lt 8) {
      $chain += [ordered]@{Type=$exception.GetType().FullName;HResult=$exception.HResult}
      $exception=$exception.InnerException
    }
    $record=[ordered]@{Schema=1;Utc=[DateTime]::UtcNow.ToString('o');Event=$Event;Phase=$script:CleanupPhase;ProcessId=$ProcessId;ProcessBits=([IntPtr]::Size*8);ScriptLine=$(if($Failure){$Failure.InvocationInfo.ScriptLineNumber}else{$null});Exceptions=$chain}
    [IO.File]::AppendAllText($script:CleanupLogPath,((ConvertTo-Json -InputObject $record -Depth 5 -Compress)+[Environment]::NewLine),[Text.UTF8Encoding]::new($false))
  } catch { }
}
function Set-EnglishAssistCleanupPhase([string]$Phase) {
  $script:CleanupPhase=$Phase
  Write-EnglishAssistCleanupDiagnostic 'Phase'
}
function Test-EnglishAssistSameProcess($Expected,$Current) {
  if (-not $Expected.CreationDate -or -not $Current.CreationDate) { throw 'A helper process creation time could not be verified.' }
  $expectedTicks=([datetime]$Expected.CreationDate).ToUniversalTime().Ticks
  $currentTicks=([datetime]$Current.CreationDate).ToUniversalTime().Ticks
  # CIM timestamps have microsecond precision; native process times use 100ns.
  $expectedTicks -= $expectedTicks % 10
  $currentTicks -= $currentTicks % 10
  $Expected.ProcessId -eq $Current.ProcessId -and $expectedTicks -eq $currentTicks
}
function Stop-EnglishAssistOwnedHelper(
  $Snapshot,[string]$OwnerSid,
  [scriptblock]$Lookup={param($Id) Get-CimInstance Win32_Process -Filter ('ProcessId='+[int]$Id) -ErrorAction Stop},
  [scriptblock]$ReadOwner={param($Process) Invoke-CimMethod -InputObject $Process -MethodName GetOwnerSid -ErrorAction Stop},
  [scriptblock]$Terminate={
    param($Process)
    $native=[Diagnostics.Process]::GetProcessById([int]$Process.ProcessId)
    try {
      # Cache the native handle before checking creation time. Kill then targets
      # that process object, never a subsequently recycled numeric PID.
      $null=$native.Handle
      $actual=[pscustomobject]@{ProcessId=$native.Id;CreationDate=$native.StartTime}
      if (-not (Test-EnglishAssistSameProcess $Process $actual)) { return 'Gone' }
      $native.Kill()
      if (-not $native.WaitForExit(5000)) { throw 'An owned helper did not exit within the cleanup deadline.' }
      'Stopped'
    } finally { $native.Dispose() }
  }
) {
  $id=[int]$Snapshot.ProcessId
  if ($id -le 0 -or -not $Snapshot.CreationDate) { throw 'Invalid helper identity.' }
  try {
    $current=& $Lookup $id
    if (-not $current -or -not (Test-EnglishAssistSameProcess $Snapshot $current)) { return 'Gone' }
    if ($current.Name -ine $Snapshot.Name -or $current.ExecutablePath -ine $Snapshot.ExecutablePath) { throw 'The helper executable identity changed.' }
    $owner=& $ReadOwner $current
    if ($owner.ReturnValue -ne 0 -or -not $owner.Sid) { throw 'The helper owner could not be verified.' }
    if ($owner.Sid -ne $OwnerSid) { return 'OtherOwner' }
    & $Terminate $current
  } catch {
    $failure=$_
    # A process can exit during owner lookup, handle acquisition or termination.
    # Suppress only proof that this precise process identity has disappeared.
    try { $remaining=& $Lookup $id } catch { throw $failure }
    if (-not $remaining -or -not (Test-EnglishAssistSameProcess $Snapshot $remaining)) { return 'Gone' }
    throw $failure
  }
}
function Remove-EnglishAssistUserTsfState(
  [scriptblock]$Exists={param($Path) Test-Path -LiteralPath $Path -ErrorAction Stop},
  [scriptblock]$Delete={param($Path) Remove-Item -LiteralPath $Path -Recurse -Force -ErrorAction Stop}
) {
  # This product registers one en-US profile under this unique CLSID. The TSF
  # disable API may retain its Enable=0 user tombstone after successful removal.
  # Never accept a caller-provided path or prune the shared TIP/SortOrder roots.
  $ownTipPath='Registry::HKEY_CURRENT_USER\Software\Microsoft\CTF\TIP\{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}'
  if (& $Exists $ownTipPath) { & $Delete $ownTipPath }
  if (& $Exists $ownTipPath) { throw 'The English Assist user TSF registration was not removed.' }
}
trap {
  Write-EnglishAssistCleanupDiagnostic 'Failure' $_
  [Console]::Error.WriteLine(('English Assist cleanup failed at {0}. Diagnostic: {1}' -f $script:CleanupPhase,$script:CleanupLogPath))
  exit 1
}
$script:CleanupPhase='PowerShellModules'
. ([IO.Path]::Combine($PSScriptRoot, 'powershell-environment.ps1'))
Initialize-EnglishAssistPowerShell
try {
  # Outside the product data tree: deleting all personal data must not recreate
  # that tree merely to record a successful cleanup. No values or text are logged.
  $taskDiagnostics=[IO.Path]::Combine([IO.Path]::GetTempPath(),'EnglishAssist-cleanup-logs')
  [IO.Directory]::CreateDirectory($taskDiagnostics) | Out-Null
  $script:CleanupLogPath=[IO.Path]::Combine($taskDiagnostics,('cleanup-{0}-{1}.jsonl' -f [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'),$PID))
} catch { }
Write-EnglishAssistCleanupDiagnostic 'Start'
Set-EnglishAssistCleanupPhase 'ValidateInstallation'
. ([IO.Path]::Combine($PSScriptRoot, 'install-state.ps1'))
$taskLayout = Assert-EnglishAssistLayout $InstallRoot $PSScriptRoot
$taskInstallRoot = $taskLayout.Root
$taskActive = $taskLayout.Payload
foreach ($taskDirectory in @($taskInstallRoot, (Join-Path $taskInstallRoot 'versions'), $taskActive, (Join-Path $taskActive 'x86'))) {
  if (((Get-Item -LiteralPath $taskDirectory -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'Refusing cleanup through a reparse-point installation directory.'
  }
}
foreach ($taskRelativeFile in @('retype_ime.dll','x86\retype_ime.dll','user-profile.ps1','cleanup-english-assist.ps1','install-state.ps1','powershell-environment.ps1')) {
  $taskProductFile = Join-Path $taskActive $taskRelativeFile
  if (-not (Test-Path -LiteralPath $taskProductFile -PathType Leaf) -or
      ((Get-Item -LiteralPath $taskProductFile -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'The expected English Assist installation files are missing or redirected; repair the installation first.'
  }
}
$taskMachineHive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, [Microsoft.Win32.RegistryView]::Registry64)
try {
  $taskMachineKey = $taskMachineHive.OpenSubKey('Software\EnglishAssist')
  if (-not $taskMachineKey) { throw 'English Assist machine installation metadata is missing.' }
  try {
    $taskMachine = [pscustomobject]@{
      ActiveDir=$taskMachineKey.GetValue('ActiveDir'); ProductId=$taskMachineKey.GetValue('ProductId')
      TipClsid=$taskMachineKey.GetValue('TipClsid'); FreshInstall=$taskMachineKey.GetValue('FreshInstall', 0)
    }
  } finally { $taskMachineKey.Dispose() }
} finally { $taskMachineHive.Dispose() }
# A missing COM view is possible after failed registration. An existing view
# must belong to this product's own version tree, never another input method.
foreach ($taskView in @([Microsoft.Win32.RegistryView]::Registry64,[Microsoft.Win32.RegistryView]::Registry32)) {
  $taskHive = [Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine, $taskView)
  try {
    $taskClass = $taskHive.OpenSubKey('Software\Classes\CLSID\{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}\InprocServer32')
    if ($taskClass) {
      try { $taskImage = Get-EnglishAssistCanonicalPath ([string]$taskClass.GetValue('')) }
      finally { $taskClass.Dispose() }
      if ([IO.Path]::GetFileName($taskImage) -ne 'retype_ime.dll') { throw 'Unexpected English Assist COM image.' }
      $taskImageDirectory = [IO.Path]::GetDirectoryName($taskImage)
      if ($taskView -eq [Microsoft.Win32.RegistryView]::Registry32) {
        if ([IO.Path]::GetFileName($taskImageDirectory) -ne 'x86') { throw 'Unexpected x86 English Assist COM directory.' }
        $taskImageDirectory = [IO.Path]::GetDirectoryName($taskImageDirectory)
      }
      $null = Assert-EnglishAssistLayout $taskInstallRoot $taskImageDirectory
    }
  } finally { $taskHive.Dispose() }
}
$taskOwnerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$taskInputProfile = Resolve-EnglishAssistInputProfile $taskOwnerSid
$taskInputPreferences = Get-EnglishAssistInputPreferenceLayout $taskInputProfile
if ($taskInputPreferences.Exists) { $null = Read-EnglishAssistInputPreferenceImage $taskInputPreferences.File }
$taskOwner = $null
if (Test-Path -LiteralPath 'HKCU:\Software\EnglishAssist') {
  $taskOwner = Get-ItemProperty -LiteralPath 'HKCU:\Software\EnglishAssist' -ErrorAction Stop
}
$taskLocal = [IO.Path]::GetFullPath($env:LOCALAPPDATA).TrimEnd('\')
$taskDataRoot = [IO.Path]::GetFullPath((Join-Path $taskLocal 'EnglishAssist'))
if (Test-Path -LiteralPath $taskDataRoot) {
  if (((Get-Item -LiteralPath $taskDataRoot -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
    throw 'Refusing cleanup through a redirected personal data root.'
  }
}
$taskProfileState = & (Join-Path $PSScriptRoot 'user-profile.ps1') -InspectOnly
$taskMode = Get-EnglishAssistCleanupMode -CurrentSid $taskOwnerSid -InstallRoot $taskInstallRoot -PayloadDirectory $taskActive -Machine $taskMachine -Owner $taskOwner -HasUserData ((Test-Path -LiteralPath $taskDataRoot) -or $taskInputPreferences.DirectoryExists) -ProfileListed ([bool]$taskProfileState.Listed) -ProfileEnabled ([bool]$taskProfileState.Enabled)
if ($taskMode -eq 'MachineOnly') {
  Write-Output 'No user enrollment or data exists for this failed first installation. User cleanup skipped; machine files and registration remain for the Inno uninstaller.'
  return
}
Set-EnglishAssistCleanupPhase 'BlockHelperRestart'
# Only after ownership/layout verification. Keep owner metadata on every error
# so retry/re-enrollment can recover without authorizing ownerless data removal.
Set-ItemProperty -LiteralPath 'HKCU:\Software\EnglishAssist' -Name 'Uninstalling' -Type DWord -Value 1 -ErrorAction Stop
Set-EnglishAssistCleanupPhase 'RemoveUserKeyboard'
& (Join-Path $PSScriptRoot 'user-profile.ps1') -Uninstall
if (-not $?) { throw 'English keyboard removal failed.' }
Set-EnglishAssistCleanupPhase 'RemoveOwnUserTsfState'
Remove-EnglishAssistUserTsfState
# Only stop this product's helper processes, in its verified install tree, for this user.
$taskHelpers = @('retype-learning-host.exe','retype-ai-host.exe','retype-sync-host.exe','retype.exe')
Set-EnglishAssistCleanupPhase 'StopOwnedHelpers'
$taskProcesses = Get-CimInstance Win32_Process | Where-Object {
  $taskHelpers -contains $_.Name -and $_.ExecutablePath -and
  $_.ExecutablePath.StartsWith($taskInstallRoot + '\versions\', [StringComparison]::OrdinalIgnoreCase)
}
foreach ($taskProcess in $taskProcesses) {
  $taskOutcome=Stop-EnglishAssistOwnedHelper $taskProcess $taskOwnerSid
  Write-EnglishAssistCleanupDiagnostic $taskOutcome $null ([int]$taskProcess.ProcessId)
}

Set-EnglishAssistCleanupPhase 'RemoveProductCredentials'
if (-not ('EnglishAssist.CredentialCleanup' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
namespace EnglishAssist {
  public static class CredentialCleanup {
    [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)]
    private struct CredentialHeader { public uint Flags; public uint Type; public IntPtr TargetName; }
    [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern bool CredEnumerateW(string filter, uint flags, out uint count, out IntPtr credentials);
    [DllImport("advapi32.dll", CharSet=CharSet.Unicode, SetLastError=true)]
    private static extern bool CredDeleteW(string target, uint type, uint flags);
    [DllImport("advapi32.dll")] private static extern void CredFree(IntPtr pointer);
    public static void RemoveProductCredentials() {
      uint count; IntPtr list;
      if (!CredEnumerateW("EnglishAssist/*", 0, out count, out list)) {
        int error = Marshal.GetLastWin32Error();
        if (error == 1168) return;
        throw new Win32Exception(error);
      }
      try {
        if (count > 10000) throw new InvalidOperationException("Unexpected credential count.");
        for (int i = 0; i < count; i++) {
          var credential = (CredentialHeader)Marshal.PtrToStructure(
            Marshal.ReadIntPtr(list, i * IntPtr.Size), typeof(CredentialHeader));
          string target = Marshal.PtrToStringUni(credential.TargetName);
          if (target == null || !target.StartsWith("EnglishAssist/", StringComparison.Ordinal))
            throw new InvalidOperationException("Unexpected credential namespace.");
          if (!CredDeleteW(target, credential.Type, 0)) {
            int error = Marshal.GetLastWin32Error();
            if (error != 1168) throw new Win32Exception(error);
          }
        }
      } finally { CredFree(list); }
    }
  }
}
'@
}
[EnglishAssist.CredentialCleanup]::RemoveProductCredentials()
if ([IO.Path]::GetDirectoryName($taskDataRoot) -ne $taskLocal) {
  throw 'Unexpected personal data directory.'
}
function Remove-OwnedData([string]$taskTarget) {
  $taskResolved = [IO.Path]::GetFullPath($taskTarget)
  if ($taskResolved -ne $taskDataRoot -and
      -not $taskResolved.StartsWith($taskDataRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'Refusing deletion outside EnglishAssist personal data.'
  }
  if (Test-Path -LiteralPath $taskResolved) {
    $taskItem = Get-Item -LiteralPath $taskResolved -Force
    if (($taskItem.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
      throw 'Refusing deletion of a reparse-point data directory.'
    }
    $taskLinks = Get-ChildItem -LiteralPath $taskResolved -Recurse -Force |
      Where-Object { ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0 }
    if ($taskLinks) { throw 'Refusing recursive deletion through a reparse point.' }
    Remove-Item -LiteralPath $taskResolved -Recurse -Force
  }
}
if ($DeletePersonalData) {
  Set-EnglishAssistCleanupPhase 'RemoveAllPersonalData'
  Remove-EnglishAssistInputPreferences $taskInputProfile
  Remove-OwnedData $taskDataRoot
} else {
  Set-EnglishAssistCleanupPhase 'RemoveRequestedProductData'
  foreach ($taskChild in @('ai','sync','logs','statistics','updates','dict-packs','dictionaries')) {
    Remove-OwnedData (Join-Path $taskDataRoot $taskChild)
  }
}
$taskRunPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
Set-EnglishAssistCleanupPhase 'RemoveOwnAutorun'
if (Test-Path -LiteralPath $taskRunPath) {
  $taskRunKey = Get-Item -LiteralPath $taskRunPath -ErrorAction Stop
  if ($taskRunKey.GetValueNames() -contains 'EnglishAssist Learning') {
    Remove-ItemProperty -LiteralPath $taskRunPath -Name 'EnglishAssist Learning' -ErrorAction Stop
  }
}
if (Test-Path -LiteralPath 'HKCU:\Software\EnglishAssist') {
  Set-EnglishAssistCleanupPhase 'RemoveOwnerMetadata'
  Remove-Item -LiteralPath 'HKCU:\Software\EnglishAssist' -Recurse -Force -ErrorAction Stop
}
Set-EnglishAssistCleanupPhase 'Complete'
Write-EnglishAssistCleanupDiagnostic 'Success'
Write-Output 'English Assist user keyboard, credentials and requested personal data were removed.'
