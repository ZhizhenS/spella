<#
.SYNOPSIS
  Add/remove English Assist in the current user's Windows keyboard list (no elevation needed).
.DESCRIPTION
  Machine registration alone does not add a TIP to the Windows user language list.
  Call InstallLayoutOrTip without default/clean-install flags, preserving other keyboards.
  Run as the actual desktop user, not an alternate administrator account.
#>
[CmdletBinding()]
param([switch]$Uninstall, [switch]$Verify, [switch]$InspectOnly)
$ErrorActionPreference = 'Stop'
. ([IO.Path]::Combine($PSScriptRoot, 'powershell-environment.ps1'))
$script:EnrollmentPhase = 'Initialize'
$script:EnrollmentLogPath = $null
$script:EnrollmentStarted = [Diagnostics.Stopwatch]::StartNew()

function Write-EnglishAssistEnrollmentDiagnostic(
  [string]$Event,
  [System.Management.Automation.ErrorRecord]$Failure = $null,
  [int]$Attempt = 0
) {
  # Only fixed diagnostic fields belong here. Never serialize an ErrorRecord,
  # command line, exception message, source line, registry value or user text.
  try {
    if (-not $script:EnrollmentLogPath) { return }
    $exceptions = @()
    if ($Failure) {
      $exception = $Failure.Exception
      while ($exception -and $exceptions.Count -lt 8) {
        $exceptions += [ordered]@{
          Type=$exception.GetType().FullName; HResult=$exception.HResult
        }
        $exception = $exception.InnerException
      }
    }
    $record = [ordered]@{
      Schema=1; Utc=[DateTime]::UtcNow.ToString('o'); Event=$Event
      Phase=$script:EnrollmentPhase; Attempt=$Attempt
      ElapsedMilliseconds=$script:EnrollmentStarted.ElapsedMilliseconds
      ProcessBits=([IntPtr]::Size * 8); Os64Bit=[Environment]::Is64BitOperatingSystem
      PowerShell=$PSVersionTable.PSVersion.ToString()
      Mode=$(if ($Uninstall) {'Uninstall'} elseif ($Verify) {'Verify'} elseif ($InspectOnly) {'Inspect'} else {'Enroll'})
      Elevated=$script:EnrollmentElevated; SessionId=$script:EnrollmentSessionId
      ErrorCategory=$(if ($Failure) {[string]$Failure.CategoryInfo.Category} else {$null})
      ScriptLine=$(if ($Failure) {$Failure.InvocationInfo.ScriptLineNumber} else {$null})
      Exceptions=$exceptions
    }
    $line = ConvertTo-Json -InputObject $record -Compress -Depth 5
    [IO.File]::AppendAllText($script:EnrollmentLogPath, $line + [Environment]::NewLine,
      [Text.UTF8Encoding]::new($false))
  } catch {
    # Diagnostic I/O must never turn a failed setup into success or hide its error.
  }
}

function Set-EnglishAssistEnrollmentPhase([string]$Phase) {
  $script:EnrollmentPhase = $Phase
  Write-EnglishAssistEnrollmentDiagnostic 'Phase'
}

function Get-EnglishAssistSha256([string]$Path) {
  # Get-FileHash is a PowerShell module function and may be unavailable in
  # Setup's original-user environment. Hash directly without module autoload.
  $stream = [IO.File]::OpenRead($Path)
  try {
    $algorithm = [Security.Cryptography.SHA256]::Create()
    try {
      return [BitConverter]::ToString($algorithm.ComputeHash($stream)).Replace('-', '').ToLowerInvariant()
    } finally { $algorithm.Dispose() }
  } finally { $stream.Dispose() }
}

function Test-EnglishAssistOfflineLexicon([string]$Root) {
  $manifestPath = Join-Path $Root 'english-lexicon-v1.json'
  $imagePath = Join-Path $Root 'english-lexicon-v1.bin'
  # An old package may omit both optional metadata files. A partly copied or
  # corrupt new package must fail before publishing its dictionary directory.
  if (-not (Test-Path -LiteralPath $manifestPath) -and -not (Test-Path -LiteralPath $imagePath)) { return $false }
  foreach ($path in @($manifestPath, $imagePath)) {
    if (-not (Test-Path -LiteralPath $path -PathType Leaf) -or
        ((Get-Item -LiteralPath $path).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
      throw 'The installed offline English lexicon is incomplete or linked.'
    }
  }
  $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
  if ($manifest.schema -ne 1 -or $manifest.format -cne 'RTENLX01' -or
      $manifest.file -cne 'english-lexicon-v1.bin' -or
      $manifest.sha256 -cnotmatch '^[a-f0-9]{64}$' -or
      [long]$manifest.bytes -lt 24 -or [long]$manifest.bytes -gt 67108864 -or
      (Get-Item -LiteralPath $imagePath).Length -ne [long]$manifest.bytes -or
      (Get-EnglishAssistSha256 $imagePath) -ne $manifest.sha256) {
    throw 'The installed offline English lexicon failed manifest/size/hash verification.'
  }
  $stream = [IO.File]::OpenRead($imagePath)
  try {
    $magic = New-Object byte[] 8
    if ($stream.Read($magic, 0, 8) -ne 8 -or [Text.Encoding]::ASCII.GetString($magic) -cne 'RTENLX01') {
      throw 'The installed offline English lexicon has an unexpected format.'
    }
  } finally { $stream.Dispose() }
  return $true
}

function Wait-EnglishAssistProfileState(
  [scriptblock]$Read,
  [scriptblock]$Ready,
  [string]$FailureMessage,
  [ValidateRange(1, 20)][int]$MaxAttempts = 10,
  [ValidateRange(0, 1000)][int]$DelayMilliseconds = 300
) {
  # Retry only callers' read-only registration/TSF snapshots. Never retry ACL,
  # startup, dictionary, default-input or unrelated-keyboard operations.
  for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
    $failure = $null
    try { $state = & $Read } catch { $failure = $_ }
    if (-not $failure -and (& $Ready $state)) { return $state }
    if ($attempt -eq $MaxAttempts) {
      if ($failure) { throw $failure }
      throw $FailureMessage
    }
    Write-EnglishAssistEnrollmentDiagnostic 'Retry' $failure $attempt
    Start-Sleep -Milliseconds $DelayMilliseconds
  }
}

trap {
  Write-EnglishAssistEnrollmentDiagnostic 'Failure' $_
  [Console]::Error.WriteLine(('English Assist user setup failed at {0}. Diagnostic: {1}' -f
    $script:EnrollmentPhase, $script:EnrollmentLogPath))
  exit 1
}

Set-EnglishAssistEnrollmentPhase 'PowerShellModules'
Initialize-EnglishAssistPowerShell
Set-EnglishAssistEnrollmentPhase 'ManualWordsScope'
. (Join-Path $PSScriptRoot 'manual-words-access.ps1')
$null = Get-EnglishAssistManualWordsLayout $env:LOCALAPPDATA

# A separate directory keeps installer diagnostics apart from AppContainer logs.
# Failure to create it leaves setup's original failure behavior intact.
try {
  $diagnostics = Join-Path $env:LOCALAPPDATA 'EnglishAssist\setup-logs'
  [IO.Directory]::CreateDirectory($diagnostics) | Out-Null
  $script:EnrollmentLogPath = Join-Path $diagnostics ('user-profile-{0}-{1}.jsonl' -f
    [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'), $PID)
  $identity = [Security.Principal.WindowsIdentity]::GetCurrent()
  $principal = [Security.Principal.WindowsPrincipal]::new($identity)
  $script:EnrollmentElevated = $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
  $script:EnrollmentSessionId = [Diagnostics.Process]::GetCurrentProcess().SessionId
} catch { }
Write-EnglishAssistEnrollmentDiagnostic 'Start'

Set-EnglishAssistEnrollmentPhase 'LoadInstallState'
. (Join-Path $PSScriptRoot 'install-state.ps1')
if (-not $Uninstall -and -not $Verify -and -not $InspectOnly) {
  Set-EnglishAssistEnrollmentPhase 'OwnershipMetadata'
  $installRoot = Split-Path (Split-Path $PSScriptRoot -Parent) -Parent
  $layout = Assert-EnglishAssistLayout $installRoot $PSScriptRoot
  # Record the original desktop user before resource, COM, language-list or ACL
  # work can fail. No profile changes are attempted unless all ownership values
  # have been written successfully. A failed upgrade may safely keep this record.
  $preferences = 'HKCU:\Software\EnglishAssist'
  if (-not (Test-Path -LiteralPath $preferences)) { New-Item -Path $preferences -Force | Out-Null }
  Set-ItemProperty -LiteralPath $preferences -Name 'ProfileOwnerSid' -Value ([Security.Principal.WindowsIdentity]::GetCurrent().User.Value)
  Set-ItemProperty -LiteralPath $preferences -Name 'ProfileInstallRoot' -Value $layout.Root
  Set-ItemProperty -LiteralPath $preferences -Name 'ProfileInstallDir' -Value $layout.Payload
  Set-ItemProperty -LiteralPath $preferences -Name 'ProfileProductId' -Value $script:EnglishAssistProductId
}
$entries = @(
  @{ Language = [uint16]0x0409; Profile = 'F395A724-DC88-4E75-B3D4-9063A552B8F4'; Name = 'English Assist' }
)
foreach ($entry in $entries) {
  $entry.Tip = ('{0:X4}:{{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}}{{{1}}}' -f $entry.Language, $entry.Profile)
}
$tips = @($entries | ForEach-Object { $_.Tip })

function Get-UserTips {
  @(Get-WinUserLanguageList | ForEach-Object { $_.InputMethodTips })
}

Set-EnglishAssistEnrollmentPhase 'SnapshotKeyboards'
$before = @(Get-UserTips)
$defaultBefore = $null
if (-not $Verify -and -not $InspectOnly) {
  $defaultBefore = [string]((Get-WinDefaultInputMethodOverride).InputMethodTip)
}
if ($Uninstall) {
  Set-EnglishAssistEnrollmentPhase 'StopOwnHosts'
  $syncHost = Join-Path $PSScriptRoot 'retype-sync-host.exe'
  if (Test-Path -LiteralPath $syncHost) {
    Start-Process -FilePath $syncHost -ArgumentList '--stop' -WindowStyle Hidden -Wait | Out-Null
  }
  Remove-ItemProperty -Path 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' -Name 'EnglishAssist Learning' -ErrorAction SilentlyContinue
  $learningHost = Join-Path $PSScriptRoot 'retype-learning-host.exe'
  if (Test-Path -LiteralPath $learningHost) {
    Start-Process -FilePath $learningHost -ArgumentList '--stop' -WindowStyle Hidden -Wait | Out-Null
  }
}
if (-not $Uninstall -and -not $InspectOnly) {
  Set-EnglishAssistEnrollmentPhase 'AwaitMachineRegistration'
  foreach ($entry in $entries) {
    $languageKey = '0x{0:x8}' -f $entry.Language
    $profile = "Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\CTF\TIP\{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}\LanguageProfile\$languageKey\{$($entry.Profile)}"
    $null = Wait-EnglishAssistProfileState { Test-Path -LiteralPath $profile } { param($present) $present } `
      "Install $($entry.Name) before adding its keyboard."
  }
}
Set-EnglishAssistEnrollmentPhase 'LoadProfileInterop'
if (-not ('EnglishAssist.UserInputProfile' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace EnglishAssist {
    [ComImport, Guid("1F02B6C5-7842-4EE6-8A0B-9A24183A95CA"),
     InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IInputProcessorProfiles {
        void Register(ref Guid clsid);
        void Unregister(ref Guid clsid);
        void AddLanguageProfile(ref Guid clsid, ushort lang, ref Guid profile, IntPtr description, uint descriptionLength, IntPtr icon, uint iconLength, uint iconIndex);
        void RemoveLanguageProfile(ref Guid clsid, ushort lang, ref Guid profile);
        void EnumInputProcessorInfo(out IntPtr enumerator);
        void GetDefaultLanguageProfile(ushort lang, ref Guid category, out Guid clsid, out Guid profile);
        void SetDefaultLanguageProfile(ushort lang, ref Guid clsid, ref Guid profile);
        void ActivateLanguageProfile(ref Guid clsid, ushort lang, ref Guid profile);
        void GetActiveLanguageProfile(ref Guid clsid, out ushort lang, out Guid profile);
        void GetLanguageProfileDescription(ref Guid clsid, ushort lang, ref Guid profile, out IntPtr description);
        void GetCurrentLanguage(out ushort lang);
        void ChangeCurrentLanguage(ushort lang);
        void GetLanguageList(out IntPtr languages, out uint count);
        void EnumLanguageProfiles(ushort lang, out IntPtr enumerator);
        void EnableLanguageProfile(ref Guid clsid, ushort lang, ref Guid profile, [MarshalAs(UnmanagedType.Bool)] bool enable);
        void IsEnabledLanguageProfile(ref Guid clsid, ushort lang, ref Guid profile, [MarshalAs(UnmanagedType.Bool)] out bool enabled);
    }
    // SDK msctf.h: TF_INPUTPROCESSORPROFILE. Default native alignment is
    // essential: dwFlags is at offset 80 on x64 and 68 on x86.
    [StructLayout(LayoutKind.Sequential)]
    public struct NativeProfile {
        public uint dwProfileType;
        public ushort langid;
        public Guid clsid;
        public Guid guidProfile;
        public Guid catid;
        public IntPtr hklSubstitute;
        public uint dwCaps;
        public IntPtr hkl;
        public uint dwFlags;
    }
    [ComImport, Guid("71C6E74C-0F28-11D8-A82A-00065B84435C"),
     InterfaceType(ComInterfaceType.InterfaceIsIUnknown)]
    internal interface IInputProcessorProfileManager {
        // Enrollment never activates a host. Uninstall deactivates only this
        // product's exact profile across the current desktop session.
        [PreserveSig] int ReservedActivateProfile(uint type, ushort language, ref Guid clsid, ref Guid profile, IntPtr hkl, uint flags);
        [PreserveSig] int DeactivateProfile(uint type, ushort language, ref Guid clsid, ref Guid profile, IntPtr hkl, uint flags);
        [PreserveSig] int GetProfile(uint type, ushort language, ref Guid clsid, ref Guid profile, IntPtr hkl, out NativeProfile result);
    }
    public static class UserInputProfile {
        private const uint InputProcessor = 1;
        private static readonly Guid Tip = new Guid("9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D");
        private static readonly Guid English = new Guid("F395A724-DC88-4E75-B3D4-9063A552B8F4");
        private static Guid OwnProfile(ushort language, string profileId) {
            if (language != 0x0409) throw new ArgumentException("Only the English Assist en-US profile may be changed.");
            var profile = new Guid(profileId);
            if (profile != English) throw new ArgumentException("Only the English Assist profile may be accessed.");
            return profile;
        }
        private static void RequireSuccess(int hr, string operation) {
            if (hr != 0) throw new COMException(String.Format(
                "{0} did not succeed (HRESULT 0x{1:X8}).", operation, unchecked((uint)hr)), hr);
        }
        private static uint ModernFlags(IInputProcessorProfileManager manager, ushort language, Guid profile) {
            var clsid = Tip;
            NativeProfile actual;
            RequireSuccess(manager.GetProfile(InputProcessor, language, ref clsid, ref profile, IntPtr.Zero, out actual), "GetProfile");
            if (actual.dwProfileType != InputProcessor || actual.langid != language ||
                actual.clsid != Tip || actual.guidProfile != English)
                throw new InvalidOperationException("GetProfile returned a different input profile.");
            return actual.dwFlags;
        }
        public static bool IsEnabled(ushort language, string profileId) {
            var profile = OwnProfile(language, profileId);
            var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("33C53A50-F456-4884-B049-85FD643ECFED")));
            try {
                var profiles = (IInputProcessorProfiles)instance;
                var clsid = Tip;
                bool enabled;
                profiles.IsEnabledLanguageProfile(ref clsid, language, ref profile, out enabled);
                return enabled;
            } finally { Marshal.ReleaseComObject(instance); }
        }
        public static uint GetModernFlags(ushort language, string profileId) {
            var profile = OwnProfile(language, profileId);
            var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("33C53A50-F456-4884-B049-85FD643ECFED")));
            try {
                return ModernFlags((IInputProcessorProfileManager)instance, language, profile);
            } finally { Marshal.ReleaseComObject(instance); }
        }
        public static void DeactivateOwnProfile(ushort language, string profileId) {
            var profile = OwnProfile(language, profileId);
            var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("33C53A50-F456-4884-B049-85FD643ECFED")));
            try {
                var clsid = Tip;
                const uint ForSession = 0x20000000;
                RequireSuccess(((IInputProcessorProfileManager)instance).DeactivateProfile(
                    InputProcessor, language, ref clsid, ref profile, IntPtr.Zero, ForSession), "DeactivateProfile");
            } finally { Marshal.ReleaseComObject(instance); }
        }
        public static void SetEnabled(ushort language, string profileId, bool enabled) {
            var profile = OwnProfile(language, profileId);
            var instance = Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("33C53A50-F456-4884-B049-85FD643ECFED")));
            try {
                var profiles = (IInputProcessorProfiles)instance;
                var clsid = Tip;
                profiles.EnableLanguageProfile(ref clsid, language, ref profile, enabled);
                // Read-back uses fresh COM instances in the bounded PowerShell
                // wait below, avoiding an immediately stale manager snapshot.
                // Never activate a host or change machine/default availability.
            } finally {
                Marshal.ReleaseComObject(instance);
            }
        }
        [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
        [DllImport("input.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool InstallLayoutOrTip(string profile, uint flags);
    }
}
'@
}
# 0 = append/enable, 1 = uninstall. Never change the user's default input method.
function Get-EntryState($Entry, $ListedTips) {
  $languageKey = '0x{0:x8}' -f $Entry.Language
  $registered = Test-Path -LiteralPath ("Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\CTF\TIP\{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}\LanguageProfile\$languageKey\{$($Entry.Profile)}")
  $legacyEnabled = $false
  $modernFlags = [uint32]0
  if ($registered) {
    $legacyEnabled = [EnglishAssist.UserInputProfile]::IsEnabled($Entry.Language, $Entry.Profile)
    $modernFlags = [EnglishAssist.UserInputProfile]::GetModernFlags($Entry.Language, $Entry.Profile)
  }
  $modernEnabled = ($modernFlags -band 2) -ne 0
  # ModernEnabled is registered-profile availability. Enabled additionally
  # requires legacy user enablement; neither proves foreground activation.
  [pscustomobject]@{
    Listed=($ListedTips -contains $Entry.Tip); Registered=$registered
    LegacyEnabled=$legacyEnabled; ModernEnabled=$modernEnabled
    ModernFlags=$modernFlags; Enabled=($legacyEnabled -and $modernEnabled)
  }
}
if ($InspectOnly) {
  Set-EnglishAssistEnrollmentPhase 'InspectProfile'
  foreach ($entry in $entries) {
    Get-EntryState $entry $before
  }
  Write-EnglishAssistEnrollmentDiagnostic 'Success'
  return
}
if ($Verify) {
  Set-EnglishAssistEnrollmentPhase 'VerifyProfile'
  foreach ($entry in $entries) {
    $state = Get-EntryState $entry $before
    if (-not $state.Listed -or -not $state.Enabled) {
      throw "$($entry.Name) is missing or disabled in the legacy or modern TSF view."
    }
  }
  Write-EnglishAssistEnrollmentDiagnostic 'Success'
  return
}
if (-not $Uninstall) {
  Set-EnglishAssistEnrollmentPhase 'VerifyVocabulary'
  $baseRoot = Join-Path $PSScriptRoot 'dictionaries'
  $englishHash = 'e1098fbaa665d52788f6ed1fde2a00eb8145e459c41583f5e59f4678db7619bc'
  $englishImage = Join-Path $baseRoot ("english-$englishHash.bin")
  if (-not (Test-Path -LiteralPath $englishImage -PathType Leaf) -or
      (Get-Item -LiteralPath $englishImage).Length -ne 3044297 -or
      (Get-EnglishAssistSha256 $englishImage) -ne $englishHash) {
    throw 'The installed offline English vocabulary is missing or failed verification.'
  }
  Set-EnglishAssistEnrollmentPhase 'VerifyLexicon'
  $null = Test-EnglishAssistOfflineLexicon $baseRoot
  # TIPs in AppContainer hosts (including Windows Search) must read the same
  # input preferences as desktop apps. Share this settings key read-only;
  # preserve its existing ACL and do not grant access to sibling/subkeys.
  Set-EnglishAssistEnrollmentPhase 'PreferencesAcl'
  $preferences = 'HKCU:\Software\EnglishAssist'
  if (-not (Test-Path $preferences)) { New-Item -Path $preferences -Force | Out-Null }
  $acl = Get-Acl -Path $preferences
  $packages = [Security.Principal.SecurityIdentifier]::new('S-1-15-2-1')
  $read = [Security.AccessControl.RegistryAccessRule]::new($packages,
    [Security.AccessControl.RegistryRights]::ReadKey,
    [Security.AccessControl.InheritanceFlags]::None,
    [Security.AccessControl.PropagationFlags]::None,
    [Security.AccessControl.AccessControlType]::Allow)
  $acl.SetAccessRule($read)
  Set-Acl -Path $preferences -AclObject $acl

  Set-EnglishAssistEnrollmentPhase 'ManualWordsAcl'
  $manualWordsPath = Initialize-EnglishAssistManualWordsAccess $env:LOCALAPPDATA
  Set-ItemProperty -Path $preferences -Name 'ManualWordsPath' -Value $manualWordsPath

  # Dedicated startup-only diagnostics, including failures in AppContainer hosts.
  # No typed text, credentials or provider configuration is written here.
  Set-EnglishAssistEnrollmentPhase 'LogsAcl'
  $settingsLogs = Join-Path $env:LOCALAPPDATA 'EnglishAssist\logs'
  New-Item -ItemType Directory -Path $settingsLogs -Force | Out-Null
  $logsAcl = Get-Acl -LiteralPath $settingsLogs
  $logsWrite = [Security.AccessControl.FileSystemAccessRule]::new($packages,
    [Security.AccessControl.FileSystemRights]'Read, Write',
    [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
    [Security.AccessControl.PropagationFlags]::None,
    [Security.AccessControl.AccessControlType]::Allow)
  $logsAcl.SetAccessRule($logsWrite)
  Set-Acl -LiteralPath $settingsLogs -AclObject $logsAcl
  Set-ItemProperty -Path $preferences -Name 'SettingsLogPath' -Value $settingsLogs

  # The TIP runs inside desktop and AppContainer hosts. Give the dedicated
  # count-only directory (not the whole retype data tree) write access, and
  # publish its real user path since AppContainer LOCALAPPDATA is redirected.
  Set-EnglishAssistEnrollmentPhase 'StatisticsAcl'
  $statistics = Join-Path $env:LOCALAPPDATA 'EnglishAssist\statistics'
  New-Item -ItemType Directory -Path $statistics -Force | Out-Null
  $statsAcl = Get-Acl -LiteralPath $statistics
  $statsWrite = [Security.AccessControl.FileSystemAccessRule]::new($packages,
    [Security.AccessControl.FileSystemRights]'Read, Write',
    [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
    [Security.AccessControl.PropagationFlags]::None,
    [Security.AccessControl.AccessControlType]::Allow)
  $statsAcl.SetAccessRule($statsWrite)
  Set-Acl -LiteralPath $statistics -AclObject $statsAcl
  Set-ItemProperty -Path $preferences -Name 'StatisticsPath' -Value $statistics

  # Category dictionaries live outside versioned installs. Search/AppContainer
  # hosts must read the real profile path, but never write downloaded files.
  Set-EnglishAssistEnrollmentPhase 'DictionaryAcl'
  $dictionaryRoot = Join-Path $env:LOCALAPPDATA 'EnglishAssist\dict-packs'
  New-Item -ItemType Directory -Path $dictionaryRoot -Force | Out-Null
  $packAcl = Get-Acl -LiteralPath $dictionaryRoot
  $packRead = [Security.AccessControl.FileSystemAccessRule]::new($packages,
    [Security.AccessControl.FileSystemRights]::ReadAndExecute,
    [Security.AccessControl.InheritanceFlags]'ContainerInherit, ObjectInherit',
    [Security.AccessControl.PropagationFlags]::None,
    [Security.AccessControl.AccessControlType]::Allow)
  $packAcl.SetAccessRule($packRead)
  Set-Acl -LiteralPath $dictionaryRoot -AclObject $packAcl
  Set-ItemProperty -Path $preferences -Name 'DictionaryRoot' -Value $dictionaryRoot

  # Immutable offline vocabulary is installed under Program Files, independently of personal data.
  $baseRoot = Join-Path $PSScriptRoot 'dictionaries'
  if (-not (Test-Path -LiteralPath $baseRoot -PathType Container)) { throw 'Offline English vocabulary is missing from the installation.' }
  Set-ItemProperty -Path $preferences -Name 'BaseDictionaryRoot' -Value $baseRoot
  # No online dictionary download or Chinese base migration is needed.

  # This English-only product has no automatic learning. Remove only its
  # legacy autorun value, preserving existing learning data and other startups.
  Set-EnglishAssistEnrollmentPhase 'RemoveLegacyLearningAutorun'
  $learningRunPath = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
  if (Test-Path -LiteralPath $learningRunPath) {
    $learningRunKey = Get-Item -LiteralPath $learningRunPath -ErrorAction Stop
    if ($learningRunKey.GetValueNames() -contains 'EnglishAssist Learning') {
      Remove-ItemProperty -LiteralPath $learningRunPath -Name 'EnglishAssist Learning' -ErrorAction Stop
    }
  }
}
$flags = if ($Uninstall) { [uint32]1 } else { [uint32]0 }
foreach ($entry in $entries) {
  $languageKey = '0x{0:x8}' -f $entry.Language
  $registered = Test-Path -LiteralPath ("Registry::HKEY_LOCAL_MACHINE\SOFTWARE\Microsoft\CTF\TIP\{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}\LanguageProfile\$languageKey\{$($entry.Profile)}")
  if ($Uninstall -and $registered) {
    Set-EnglishAssistEnrollmentPhase 'DeactivateOwnProfile'
    [EnglishAssist.UserInputProfile]::DeactivateOwnProfile($entry.Language, $entry.Profile)
  }
  Set-EnglishAssistEnrollmentPhase 'InstallLayoutOrTip'
  if (-not $Uninstall -or $before -contains $entry.Tip) {
    if (-not [EnglishAssist.UserInputProfile]::InstallLayoutOrTip($entry.Tip, $flags)) {
      throw "InstallLayoutOrTip failed for $($entry.Name)."
    }
  }
  if ($registered) {
    Set-EnglishAssistEnrollmentPhase 'EnableUserProfile'
    [EnglishAssist.UserInputProfile]::SetEnabled($entry.Language, $entry.Profile, -not [bool]$Uninstall)
  } elseif (-not $Uninstall) {
    throw 'The machine English Assist profile is not registered.'
  }
}
# Verify both views with fresh COM objects, allowing bounded registration-cache
# propagation after regserver. An exhausted read/false condition still fails setup.
Set-EnglishAssistEnrollmentPhase 'AwaitUserProfile'
$snapshot = Wait-EnglishAssistProfileState {
  $listed = @(Get-UserTips)
  [pscustomobject]@{ Tips=$listed; States=@($entries | ForEach-Object { Get-EntryState $_ $listed }) }
} {
  param($snapshot)
  foreach ($state in $snapshot.States) {
    if ($Uninstall) {
      if ($state.Listed -or $state.LegacyEnabled) { return $false }
    } elseif (-not $state.Listed -or -not $state.Registered -or
        -not $state.LegacyEnabled -or -not $state.ModernEnabled) {
      return $false
    }
  }
  return $true
} 'Windows has not reflected the requested keyboard list and legacy/modern TSF enablement.'
$after = @($snapshot.Tips)
Set-EnglishAssistEnrollmentPhase 'VerifyPreservedKeyboards'
$othersBefore = @($before | Where-Object { $tips -notcontains $_ } | Sort-Object)
$othersAfter = @($after | Where-Object { $tips -notcontains $_ } | Sort-Object)
if ([string]::Join(';', [string[]]$othersBefore) -ne [string]::Join(';', [string[]]$othersAfter)) {
  throw 'Other keyboard entries changed during registration.'
}
$defaultAfter = [string]((Get-WinDefaultInputMethodOverride).InputMethodTip)
if ($defaultBefore -ne $defaultAfter -and (-not $Uninstall -or $tips -notcontains $defaultBefore)) {
  throw 'An unrelated default input method changed unexpectedly.'
}
if (-not $Uninstall) {
  # A guarded legacy helper must stay blocked until enrollment has fully passed.
  # Keep this explicit DWORD present; missing/invalid marker values fail closed.
  Set-EnglishAssistEnrollmentPhase 'CompleteEnrollment'
  New-ItemProperty -LiteralPath 'HKCU:\Software\EnglishAssist' -Name 'Uninstalling' -PropertyType DWord -Value 0 -Force -ErrorAction Stop | Out-Null
}
Set-EnglishAssistEnrollmentPhase 'Complete'
Write-EnglishAssistEnrollmentDiagnostic 'Success'
Write-Host $(if ($Uninstall) { 'English Assist removed from the current user keyboard list.' } else { 'English Assist added to the current user keyboard list. Use Win+Space in a desktop app.' })
