# English Assist user cleanup. Run as the user whose English keyboard is being removed.
[CmdletBinding()]
param(
  [Parameter(Mandatory=$true)][string]$InstallRoot,
  [switch]$DeletePersonalData
)
$ErrorActionPreference = 'Stop'
$taskInstallRoot = [IO.Path]::GetFullPath($InstallRoot).TrimEnd('\')
if ([IO.Path]::GetFileName($taskInstallRoot) -ne 'EnglishAssist') {
  throw 'Refusing cleanup: installation directory must be the dedicated EnglishAssist directory.'
}
$taskActive = [IO.Path]::GetFullPath($PSScriptRoot)
if (-not $taskActive.StartsWith($taskInstallRoot + '\versions\', [StringComparison]::OrdinalIgnoreCase)) {
  throw 'Refusing cleanup outside the versioned EnglishAssist installation.'
}
$taskOwnerSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$taskOwner = Get-ItemProperty -LiteralPath 'HKCU:\Software\EnglishAssist' -ErrorAction SilentlyContinue
if (-not $taskOwner -or $taskOwner.ProfileOwnerSid -ne $taskOwnerSid -or
    $taskOwner.ProfileInstallDir -ne $taskActive) {
  throw 'Run cleanup as the original desktop user who enabled this English Assist installation. No data was deleted.'
}
# Only stop this product's helper processes, in its verified install tree, for this user.
$taskHelpers = @('retype-learning-host.exe','retype-ai-host.exe','retype-sync-host.exe','retype.exe')
$taskProcesses = Get-CimInstance Win32_Process | Where-Object {
  $taskHelpers -contains $_.Name -and $_.ExecutablePath -and
  $_.ExecutablePath.StartsWith($taskInstallRoot + '\versions\', [StringComparison]::OrdinalIgnoreCase)
}
foreach ($taskProcess in $taskProcesses) {
  $taskProcessOwner = Invoke-CimMethod -InputObject $taskProcess -MethodName GetOwnerSid
  if ($taskProcessOwner.Sid -eq $taskOwnerSid) {
    Stop-Process -Id $taskProcess.ProcessId -Force -ErrorAction Stop
  }
}
& (Join-Path $PSScriptRoot 'user-profile.ps1') -Uninstall
if (-not $?) { throw 'English keyboard removal failed.' }

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
$taskLocal = [IO.Path]::GetFullPath($env:LOCALAPPDATA).TrimEnd('\')
$taskDataRoot = [IO.Path]::GetFullPath((Join-Path $taskLocal 'EnglishAssist'))
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
  Remove-OwnedData $taskDataRoot
} else {
  foreach ($taskChild in @('ai','sync','logs','statistics','updates','dict-packs','dictionaries')) {
    Remove-OwnedData (Join-Path $taskDataRoot $taskChild)
  }
}
Remove-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'EnglishAssist Learning' -ErrorAction SilentlyContinue
Remove-Item -LiteralPath 'HKCU:\Software\EnglishAssist' -Recurse -Force -ErrorAction SilentlyContinue
Write-Output 'English Assist user keyboard, credentials and requested personal data were removed.'
