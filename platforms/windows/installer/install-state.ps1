# Pure validation helpers shared by enrollment and cleanup. Importing has no side effects.
[CmdletBinding()]
param([switch]$CheckPendingRenames,[string]$PendingPolicyInstallRoot,[string]$PendingPolicyNewPayload)
function Get-EnglishAssistInputPreferenceNames {
  # EAINPF01 v1: these indices are shared with common/user_preferences.rs.
  @('PinyinScheme','EnglishDisabled','EnglishSpellingDisabled','EnglishPinyinDisabled',
    'EnglishSegmentationDisabled','EnglishPhrasesDisabled','EnglishAutoSpaceDisabled',
    'StatusBarDisabled','StatusBarX','StatusBarY','StatusBarHasPosition','CandidateTheme',
    'CandidateFontSize','CandidateWidth','CandidateCompact','CandidateHideMeanings',
    'CandidateHidePartOfSpeech','CandidateShowPhonetics','CandidateHideKeyHints',
    'SymbolCompletionEnabled','ManualWordsGeneration','DictionaryGeneration','EnabledDictionaryPacks')
}

function Resolve-EnglishAssistInputProfile([string]$OwnerSid, [scriptblock]$ReadProfile={
  param($Sid)
  $hive=[Microsoft.Win32.RegistryKey]::OpenBaseKey([Microsoft.Win32.RegistryHive]::LocalMachine,[Microsoft.Win32.RegistryView]::Registry64)
  try {
    $key=$hive.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\'+$Sid)
    if (-not $key) { throw 'The desktop user profile registration is missing.' }
    try { $key.GetValue('ProfileImagePath',$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames) }
    finally { $key.Dispose() }
  } finally { $hive.Dispose() }
}) {
  if ($OwnerSid -notmatch '^S-1-5-\d+(?:-\d+)+$') { throw 'Invalid desktop user SID.' }
  $profile=& $ReadProfile $OwnerSid
  if (-not ($profile -is [string]) -or -not $profile) { throw 'The desktop user profile path is missing.' }
  [Environment]::ExpandEnvironmentVariables($profile)
}

function Get-EnglishAssistInputPreferenceAttributes([string]$Path) {
  try { return [long][IO.File]::GetAttributes($Path) }
  catch {
    $exception=$_.Exception
    while ($exception.InnerException) { $exception=$exception.InnerException }
    if ($exception -is [IO.FileNotFoundException] -or $exception -is [IO.DirectoryNotFoundException]) { return -1 }
    throw
  }
}

function Get-EnglishAssistInputPreferenceLayout([string]$ProfileDirectory,
  [scriptblock]$ReadAttributes={param($Path) Get-EnglishAssistInputPreferenceAttributes $Path}) {
  if ($ProfileDirectory -notmatch '^[A-Za-z]:\\' -or $ProfileDirectory.Substring(2).Contains(':')) {
    throw 'Shared input preferences require a local absolute desktop profile.'
  }
  $profile=[IO.Path]::GetFullPath($ProfileDirectory).TrimEnd('\')
  if ($profile.Length -le 3 -or $profile -ine $ProfileDirectory.TrimEnd('\')) { throw 'Unexpected desktop profile path.' }
  $cursor=$profile
  while ($cursor) {
    $attributes=& $ReadAttributes $cursor
    if ($attributes -lt 0 -or ($attributes -band 0x10) -eq 0 -or ($attributes -band 0x400) -ne 0) {
      throw 'Refusing shared input preferences through a missing, linked or non-directory profile.'
    }
    $cursor=[IO.Path]::GetDirectoryName($cursor)
  }
  $directory=[IO.Path]::Combine($profile,'.EnglishAssist')
  $file=[IO.Path]::Combine($directory,'input-preferences-v1.bin')
  $attributes=& $ReadAttributes $directory
  if ($attributes -ge 0 -and (($attributes -band 0x10) -eq 0 -or ($attributes -band 0x400) -ne 0)) {
    throw 'Refusing a redirected shared input preferences directory.'
  }
  $fileAttributes=& $ReadAttributes $file
  if ($fileAttributes -ge 0 -and (($fileAttributes -band 0x410) -ne 0)) {
    throw 'Refusing a linked or non-file shared input preferences store.'
  }
  [pscustomobject]@{Profile=$profile;Directory=$directory;File=$file;Exists=($fileAttributes -ge 0);DirectoryExists=($attributes -ge 0)}
}

function Assert-EnglishAssistInputPreferenceImage([byte[]]$Bytes) {
  if ($Bytes.Length -ne 288 -or [Text.Encoding]::ASCII.GetString($Bytes,0,8) -cne 'EAINPF01' -or
      [BitConverter]::ToUInt32($Bytes,8) -ne 1 -or [BitConverter]::ToUInt32($Bytes,12) -ne 32) {
    throw 'The shared input preferences store has an invalid header or length; it was preserved.'
  }
  foreach ($index in 16..31) { if ($Bytes[$index] -ne 0) { throw 'Unexpected shared input preferences header flags.' } }
  for ($index=0;$index -lt 32;$index++) {
    $have=[BitConverter]::ToUInt32($Bytes,32+$index*8)
    $value=[BitConverter]::ToUInt32($Bytes,36+$index*8)
    # Known slots publish value before have=1. A concurrent snapshot may see
    # have=0/nonzero value; that value is ignored until the atomic publish.
    if ($have -gt 1 -or ($index -ge 23 -and ($have -ne 0 -or $value -ne 0))) {
      throw 'The shared input preferences store has an invalid slot; it was preserved.'
    }
  }
}

function Read-EnglishAssistInputPreferenceImage([string]$Path) {
  $stream=[IO.File]::Open($Path,[IO.FileMode]::Open,[IO.FileAccess]::Read,([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
  try {
    if ($stream.Length -ne 288) { throw 'The shared input preferences store has an invalid length; it was preserved.' }
    $bytes=New-Object byte[] 288
    $offset=0
    while ($offset -lt 288) {
      $count=$stream.Read($bytes,$offset,288-$offset)
      if ($count -eq 0) { throw 'The shared input preferences store could not be read completely.' }
      $offset+=$count
    }
    Assert-EnglishAssistInputPreferenceImage $bytes
    return ,$bytes
  } finally { $stream.Dispose() }
}

function Initialize-EnglishAssistInputPreferences([string]$ProfileDirectory,
  [scriptblock]$ReadDword={param($Name)
    $key=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Software\EnglishAssist')
    if (-not $key) { return $null }
    try {
      $value=$key.GetValue($Name,$null,[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
      if ($null -eq $value) { return $null }
      if ($key.GetValueKind($Name) -ne [Microsoft.Win32.RegistryValueKind]::DWord) { throw 'A legacy input preference is not a DWORD.' }
      # Registry DWORDs are returned as signed Int32; preserve every bit, even -1.
      [BitConverter]::ToUInt32([BitConverter]::GetBytes([int32]$value),0)
    } finally { $key.Dispose() }
  }) {
  $layout=Get-EnglishAssistInputPreferenceLayout $ProfileDirectory
  if ($layout.Exists) { $null=Read-EnglishAssistInputPreferenceImage $layout.File; return $layout.File }
  $bytes=New-Object byte[] 288
  [Array]::Copy([Text.Encoding]::ASCII.GetBytes('EAINPF01'),0,$bytes,0,8)
  [Array]::Copy([BitConverter]::GetBytes([uint32]1),0,$bytes,8,4)
  [Array]::Copy([BitConverter]::GetBytes([uint32]32),0,$bytes,12,4)
  $names=@(Get-EnglishAssistInputPreferenceNames)
  for ($index=0;$index -lt $names.Count;$index++) {
    $value=& $ReadDword $names[$index]
    if ($null -ne $value) {
      [Array]::Copy([BitConverter]::GetBytes([uint32]1),0,$bytes,32+$index*8,4)
      [Array]::Copy([BitConverter]::GetBytes([uint32]$value),0,$bytes,36+$index*8,4)
    }
  }
  Assert-EnglishAssistInputPreferenceImage $bytes
  $null=[IO.Directory]::CreateDirectory($layout.Directory)
  $layout=Get-EnglishAssistInputPreferenceLayout $ProfileDirectory
  # Never replace/rename an existing store: older hosts may still map its file object.
  if ($layout.Exists) { $null=Read-EnglishAssistInputPreferenceImage $layout.File; return $layout.File }
  $stream=[IO.File]::Open($layout.File,[IO.FileMode]::CreateNew,[IO.FileAccess]::Write,[IO.FileShare]::None)
  try { $stream.Write($bytes,0,$bytes.Length); $stream.Flush($true) } finally { $stream.Dispose() }
  $null=Read-EnglishAssistInputPreferenceImage $layout.File
  $layout.File
}

function Remove-EnglishAssistInputPreferences([string]$ProfileDirectory) {
  $layout=Get-EnglishAssistInputPreferenceLayout $ProfileDirectory
  if ($layout.Exists) {
    $null=Read-EnglishAssistInputPreferenceImage $layout.File
    # A sharing error is a real failure. Never queue this fixed name for reboot:
    # a later installation could otherwise lose its newly created preferences.
    [IO.File]::Delete($layout.File)
  }
  $layout=Get-EnglishAssistInputPreferenceLayout $ProfileDirectory
  if ($layout.DirectoryExists -and [IO.Directory]::GetFileSystemEntries($layout.Directory).Length -eq 0) {
    [IO.Directory]::Delete($layout.Directory,$false)
  }
}

if ($CheckPendingRenames) {
  . ([IO.Path]::Combine($PSScriptRoot, 'powershell-environment.ps1'))
  Initialize-EnglishAssistPowerShell
}
$script:EnglishAssistProductId = '{F981F16C-7472-4F3F-99C5-7C0E47956624}'
$script:EnglishAssistTipClsid = '{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}'

function Get-EnglishAssistCanonicalPath([string]$Path) {
  if ([string]::IsNullOrWhiteSpace($Path) -or $Path -notmatch '^[A-Za-z]:[\\/]') {
    throw 'An absolute English Assist path is required.'
  }
  [IO.Path]::GetFullPath($Path).TrimEnd('\')
}
function Assert-EnglishAssistLayout(
  [string]$InstallRoot,
  [string]$PayloadDirectory,
  [string]$ProgramFilesDirectory = $(if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles })
) {
  $root = Get-EnglishAssistCanonicalPath $InstallRoot
  $expected = Get-EnglishAssistCanonicalPath (Join-Path $ProgramFilesDirectory 'EnglishAssist')
  if ($root -ne $expected) {
    throw 'English Assist must use its dedicated Program Files directory.'
  }
  $payload = Get-EnglishAssistCanonicalPath $PayloadDirectory
  if ([IO.Path]::GetDirectoryName($payload) -ne (Join-Path $root 'versions')) {
    throw 'The payload must be a direct child of the EnglishAssist versions directory.'
  }
  [pscustomobject]@{ Root=$root; Payload=$payload }
}
function Get-EnglishAssistCleanupMode(
  [string]$CurrentSid,
  [string]$InstallRoot,
  [string]$PayloadDirectory,
  [object]$Machine,
  [object]$Owner,
  [bool]$HasUserData,
  [bool]$ProfileListed,
  [bool]$ProfileEnabled,
  [string]$ProgramFilesDirectory = $(if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles })
) {
  $layout = Assert-EnglishAssistLayout $InstallRoot $PayloadDirectory $ProgramFilesDirectory
  if (-not $Machine -or $Machine.ProductId -ne $script:EnglishAssistProductId -or
      $Machine.TipClsid -ne $script:EnglishAssistTipClsid -or
      (Get-EnglishAssistCanonicalPath $Machine.ActiveDir) -ne $layout.Payload) {
    throw 'The machine installation identity does not match English Assist.'
  }
  if ($Owner) {
    if ($Owner.ProfileOwnerSid -ne $CurrentSid) {
      throw 'Cleanup must run as the user who enabled English Assist.'
    }
    # A failed upgrade may leave the previous version's user registration.
    # It is accepted only when that version belongs to the same dedicated root.
    $null = Assert-EnglishAssistLayout $layout.Root $Owner.ProfileInstallDir $ProgramFilesDirectory
    if ($Owner.ProfileInstallRoot -and
        (Get-EnglishAssistCanonicalPath $Owner.ProfileInstallRoot) -ne $layout.Root) {
      throw 'The user registration belongs to another installation.'
    }
    if ($Owner.ProfileProductId -and $Owner.ProfileProductId -ne $script:EnglishAssistProductId) {
      throw 'The user registration belongs to another product.'
    }
    return 'Full'
  }
  # Only the failure of a first machine installation may have no user metadata.
  # In this state no user's profile, data, processes or credentials are touched.
  if ([int]$Machine.FreshInstall -eq 1 -and -not $HasUserData -and
      -not $ProfileListed -and -not $ProfileEnabled) {
    return 'MachineOnly'
  }
  throw 'User ownership is uncertain; repair enrollment as the original user before cleanup.'
}

function ConvertFrom-EnglishAssistPendingRenameValue([AllowEmptyString()][string]$Raw) {
  if ([string]::IsNullOrEmpty($Raw)) { return }
  $nul=[char]0
  if ($Raw.Length -lt 2 -or $Raw[$Raw.Length-1] -ne $nul -or $Raw[$Raw.Length-2] -ne $nul) {
    throw 'The pending-rename value is not a terminated MULTI_SZ.'
  }
  $position=0
  while ($position -lt $Raw.Length) {
    $end=$Raw.IndexOf($nul,$position)
    if ($end -lt 0) { throw 'The pending-rename source is not terminated.' }
    $source=$Raw.Substring($position,$end-$position); $position=$end+1
    if ($source.Length -eq 0) {
      if ($Raw.Substring($position).Trim([char[]]@($nul)).Length -ne 0) { throw 'Empty pending source before more entries.' }
      break
    }
    if ($position -ge $Raw.Length) { throw 'Missing pending destination.' }
    $end=$Raw.IndexOf($nul,$position)
    if ($end -lt 0) { throw 'Unterminated pending destination.' }
    $destination=$Raw.Substring($position,$end-$position); $position=$end+1
    if ($source.Length -gt 32767 -or $destination.Length -gt 32767) { throw 'Pending path exceeds native limits.' }
    [pscustomobject]@{ Source=$source; Destination=$destination }
  }
}
function ConvertTo-EnglishAssistPendingPath([string]$Path) {
  $value=$Path
  # Exact spelling observed in our Inno Setup 6.7.3 uninstall queue snapshot:
  # work/research/lifecycle-final/pending-own-deletes.json. The public API does
  # not define this marker's flags; do not generalize it to arbitrary *numbers.
  if ($value.StartsWith('*1\??\')) { $value=$value.Substring(2) }
  # ! is a destination replacement marker. Removal here is for ownership detection only.
  if ($value.StartsWith('!')) { $value=$value.Substring(1) }
  if ($value.StartsWith('\??\') -or $value.StartsWith('\\?\')) { $value=$value.Substring(4) }
  $value.Replace('/','\')
}
function Test-EnglishAssistPendingTouchesRoot([string]$Path,[string]$Root,[string[]]$RootAliases=@()) {
  if ([string]::IsNullOrEmpty($Path)) { return $false }
  $value=ConvertTo-EnglishAssistPendingPath $Path
  $comparison=[StringComparison]::OrdinalIgnoreCase
  if ($value.Equals($Root,$comparison) -or $value.StartsWith($Root+'\',$comparison)) { return $true }
  foreach ($alias in $RootAliases) {
    if ($value.Equals($alias,$comparison) -or $value.StartsWith($alias+'\',$comparison)) { return $true }
  }
  if ($value -match '^[A-Za-z]:\\') {
    try {
      $canonical=[IO.Path]::GetFullPath($value).TrimEnd('\')
      if ($canonical.Equals($Root,$comparison) -or $canonical.StartsWith($Root+'\',$comparison)) { return $true }
      foreach ($alias in $RootAliases) {
        if ($canonical.Equals($alias,$comparison) -or $canonical.StartsWith($alias+'\',$comparison)) { return $true }
      }
      return $false
    } catch { return $value.IndexOf($Root,$comparison) -ge 0 }
  }
  # Unknown NT aliases cannot safely establish a product-volume mapping.
  return $value.IndexOf($Root.Substring(2),$comparison) -ge 0
}
function Test-EnglishAssistVersionDirectoryName([string]$Name) {
  if ($Name -notmatch '^(?<version>\d{1,5}\.\d{1,5}\.\d{1,5}(?:\.\d{1,5})?)-(?<stamp>\d{14})-(?<nonce>\d{1,6})$') { return $false }
  $versionText=$Matches.version; $stamp=$Matches.stamp
  foreach ($part in $versionText.Split('.')) { if ([int]$part -gt 65535) { return $false } }
  $parsed=[datetime]::MinValue
  [datetime]::TryParseExact($stamp,'yyyyMMddHHmmss',[Globalization.CultureInfo]::InvariantCulture,[Globalization.DateTimeStyles]::None,[ref]$parsed)
}
function Test-EnglishAssistPendingRenamePolicy(
  [string]$InstallRoot,[string]$NewPayloadDirectory,[object[]]$Entries=@(),[string[]]$ReparsePoints=@(),
  [bool]$NewPayloadExists=$false,
  [string]$ProgramFilesDirectory=$(if ($env:ProgramW6432) { $env:ProgramW6432 } else { $env:ProgramFiles }),
  [string[]]$RootAliases=@(),
  [string[]]$VerifiedDirectories=@()
) {
  $layout=Assert-EnglishAssistLayout $InstallRoot $NewPayloadDirectory $ProgramFilesDirectory
  if ($NewPayloadExists) { throw 'The new payload directory is already in use.' }
  if (-not (Test-EnglishAssistVersionDirectoryName ([IO.Path]::GetFileName($layout.Payload)))) { throw 'New payload lacks a unique generated version suffix.' }
  $comparison=[StringComparison]::OrdinalIgnoreCase
  foreach ($link in $ReparsePoints) {
    $linkPath=Get-EnglishAssistCanonicalPath $link
    if ($layout.Payload.Equals($linkPath,$comparison) -or $layout.Payload.StartsWith($linkPath+'\',$comparison)) { throw 'New payload traverses a reparse point.' }
  }
  foreach ($entry in $Entries) {
    if (-not $entry -or -not $entry.PSObject.Properties['Source'] -or -not $entry.PSObject.Properties['Destination'] -or $null -eq $entry.Source -or $null -eq $entry.Destination) { throw 'Incomplete pending source/destination pair.' }
    $source=[string]$entry.Source; $destination=[string]$entry.Destination
    if (-not (Test-EnglishAssistPendingTouchesRoot $source $layout.Root $RootAliases) -and
        -not (Test-EnglishAssistPendingTouchesRoot $destination $layout.Root $RootAliases)) { continue }
    if ($destination.Length -ne 0) { throw 'Pending rename could alter English Assist.' }
    $sourcePath=ConvertTo-EnglishAssistPendingPath $source
    if ($source.StartsWith('!') -or $source.Contains('/') -or
        -not ($sourcePath.Equals($layout.Root,$comparison) -or $sourcePath.StartsWith($layout.Root+'\',$comparison))) { throw 'Unsupported pending product path spelling.' }
    if (-not $sourcePath.Equals(([IO.Path]::GetFullPath($sourcePath).TrimEnd('\')),$comparison)) { throw 'Pending product path is not canonical.' }
    if ($sourcePath.Equals($layout.Payload,$comparison) -or $sourcePath.StartsWith($layout.Payload+'\',$comparison)) { throw 'Pending deletion targets the new payload.' }
    $directory=$false
    if ($sourcePath.Equals($layout.Root,$comparison) -or $sourcePath.Equals((Join-Path $layout.Root 'versions'),$comparison)) {
      $directory=$true
    } else {
      $relative=$sourcePath.Substring($layout.Root.Length+1)
      if ($relative -match '^versions\\(?<version>[^\\]+)\\(?:x86\\)?retype_ime\.dll$') {
        $version=$Matches.version
        if ($VerifiedDirectories -contains $sourcePath) { throw 'A pending TIP file path currently names a directory.' }
      } elseif ($relative -match '^versions\\(?<version>[^\\]+)(?:\\(?:x86|dictionaries|settings))?$') {
        $version=$Matches.version; $directory=$true
      } else { throw 'Pending deletion is not an old TIP DLL or a known product directory.' }
      if (-not (Test-EnglishAssistVersionDirectoryName $version)) { throw 'Invalid pending version directory.' }
    }
    # MoveFileExW(NULL, MOVEFILE_DELAY_UNTIL_REBOOT) removes a directory only
    # when EMPTY at reboot; it is never a recursive delete. This safely allows
    # the product/versions parents to remain queued while a fresh payload makes
    # them nonempty. Require native attributes to confirm a directory today.
    # https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-movefileexw
    if ($directory -and $VerifiedDirectories -notcontains $sourcePath) { throw 'Pending product directory type is unverified or missing.' }
    foreach ($link in $ReparsePoints) {
      $linkPath=Get-EnglishAssistCanonicalPath $link
      if ($sourcePath.Equals($linkPath,$comparison) -or $sourcePath.StartsWith($linkPath+'\',$comparison)) { throw 'Pending product deletion traverses a reparse point.' }
    }
  }
  return $true
}
function Get-EnglishAssistPendingRenameRawValue {
  if (-not ('EnglishAssist.PendingRenameReader' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;
namespace EnglishAssist {
  public static class PendingRenameReader {
    [DllImport("advapi32.dll",CharSet=CharSet.Unicode,ExactSpelling=true)]
    private static extern int RegOpenKeyExW(IntPtr key,string subkey,uint options,uint access,out IntPtr result);
    [DllImport("advapi32.dll",CharSet=CharSet.Unicode,ExactSpelling=true)]
    private static extern int RegQueryValueExW(IntPtr key,string name,IntPtr reserved,out uint type,byte[] data,ref uint bytes);
    [DllImport("advapi32.dll",ExactSpelling=true)] private static extern int RegCloseKey(IntPtr key);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,ExactSpelling=true,SetLastError=true)]
    private static extern uint GetFileAttributesW(string path);
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode,ExactSpelling=true,SetLastError=true)]
    private static extern uint GetShortPathNameW(string path,StringBuilder output,uint characters);
    public static string ShortPathOrMissing(string path) {
      var output=new StringBuilder(32768);
      uint count=GetShortPathNameW(path,output,(uint)output.Capacity);
      if (count>0 && count<output.Capacity) return output.ToString();
      if (count>=output.Capacity) throw new InvalidOperationException("Short path exceeded its bound.");
      int error=Marshal.GetLastWin32Error();
      if (error==2 || error==3) return String.Empty;
      throw new Win32Exception(error);
    }
    public static uint AttributesOrMissing(string path) {
      uint attributes=GetFileAttributesW(path);
      if (attributes!=0xFFFFFFFF) return attributes;
      int error=Marshal.GetLastWin32Error();
      if (error==2 || error==3) return 0;
      throw new Win32Exception(error);
    }
    public static string Read() {
      IntPtr key;
      int error=RegOpenKeyExW(new IntPtr(unchecked((int)0x80000002)),"SYSTEM\\CurrentControlSet\\Control\\Session Manager",0,0x101,out key);
      if (error!=0) throw new Win32Exception(error);
      try {
        for (int attempt=0;attempt<3;attempt++) {
          uint type; uint bytes=0;
          error=RegQueryValueExW(key,"PendingFileRenameOperations",IntPtr.Zero,out type,null,ref bytes);
          if (error==2) return String.Empty;
          if (error!=0) throw new Win32Exception(error);
          if (type!=7 || bytes>2*1024*1024 || bytes%2!=0) throw new InvalidOperationException("Invalid pending-rename registry data.");
          if (bytes==0) return String.Empty;
          byte[] data=new byte[bytes];
          error=RegQueryValueExW(key,"PendingFileRenameOperations",IntPtr.Zero,out type,data,ref bytes);
          if (error==2) return String.Empty;
          if (error==234) continue;
          if (error!=0) throw new Win32Exception(error);
          if (type!=7 || bytes>data.Length || bytes%2!=0) throw new InvalidOperationException("Pending-rename data changed.");
          return new UnicodeEncoding(false,false,true).GetString(data,0,(int)bytes);
        }
        throw new InvalidOperationException("Pending-rename data kept changing.");
      } finally { RegCloseKey(key); }
    }
  }
}
'@
  }
  [EnglishAssist.PendingRenameReader]::Read()
}
function Assert-EnglishAssistPendingRenameSafety([string]$InstallRoot,[string]$NewPayloadDirectory) {
  $layout=Assert-EnglishAssistLayout $InstallRoot $NewPayloadDirectory
  $entries=@(ConvertFrom-EnglishAssistPendingRenameValue (Get-EnglishAssistPendingRenameRawValue))
  if ([EnglishAssist.PendingRenameReader]::AttributesOrMissing($layout.Payload) -ne 0) {
    throw 'The new payload directory already exists; restart Setup to choose a fresh suffix.'
  }
  $programFiles=[IO.Path]::GetDirectoryName($layout.Root)
  $shortProgramFiles=[EnglishAssist.PendingRenameReader]::ShortPathOrMissing($programFiles)
  $shortRoot=[EnglishAssist.PendingRenameReader]::ShortPathOrMissing($layout.Root)
  $aliases=@($layout.Root)
  if ($shortProgramFiles) { $aliases += Join-Path $shortProgramFiles 'EnglishAssist' }
  if ($shortRoot) {
    $aliases += $shortRoot
    $aliases += Join-Path $programFiles ([IO.Path]::GetFileName($shortRoot))
  }
  $aliases=@($aliases | Sort-Object -Unique)
  $verifiedDirectories=@(foreach ($entry in $entries) {
    $path=ConvertTo-EnglishAssistPendingPath $entry.Source
    if (($path.Equals($layout.Root,[StringComparison]::OrdinalIgnoreCase) -or $path.StartsWith($layout.Root+'\',[StringComparison]::OrdinalIgnoreCase)) -and
        $path.Equals(([IO.Path]::GetFullPath($path).TrimEnd('\')),[StringComparison]::OrdinalIgnoreCase)) {
      $attributes=[EnglishAssist.PendingRenameReader]::AttributesOrMissing($path)
      if (($attributes -band 0x10) -ne 0 -and ($attributes -band 0x400) -eq 0) { $path }
    }
  })
  $null=Test-EnglishAssistPendingRenamePolicy $layout.Root $layout.Payload $entries -RootAliases $aliases -VerifiedDirectories $verifiedDirectories
  $candidates=@($layout.Root,(Join-Path $layout.Root 'versions'),$layout.Payload)
  foreach ($entry in $entries) {
    if (Test-EnglishAssistPendingTouchesRoot $entry.Source $layout.Root) {
      $cursor=ConvertTo-EnglishAssistPendingPath $entry.Source
      while ($cursor.Length -ge $layout.Root.Length) {
        $candidates+=$cursor
        if ($cursor -eq $layout.Root) { break }
        $cursor=[IO.Path]::GetDirectoryName($cursor)
      }
    }
  }
  $links=@(foreach ($path in ($candidates | Sort-Object -Unique)) {
    # GetFileAttributes reads the link itself, including dangling links.
    if (([EnglishAssist.PendingRenameReader]::AttributesOrMissing($path) -band 0x400) -ne 0) { $path }
  })
  $null=Test-EnglishAssistPendingRenamePolicy $layout.Root $layout.Payload $entries $links -RootAliases $aliases -VerifiedDirectories $verifiedDirectories
}
if ($CheckPendingRenames) {
  # Read-only pre-install guard: scheduled operations are never added or removed.
  Assert-EnglishAssistPendingRenameSafety $PendingPolicyInstallRoot $PendingPolicyNewPayload
}


