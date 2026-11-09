# Read-only system enrollment snapshot. Does not activate a thread/profile, change
# a language/default keyboard, register a DLL, or read window titles/input text.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
if (-not ('EnglishAssistSystemSnapshot.Native' -as [type])) {
  Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
namespace EnglishAssistSystemSnapshot {
 [StructLayout(LayoutKind.Sequential)] public struct Profile {
  public uint Type; public ushort Language; public Guid Clsid; public Guid Id; public Guid Category;
  public IntPtr Substitute; public uint Caps; public IntPtr Layout; public uint Flags;
 }
 [StructLayout(LayoutKind.Sequential, CharSet=CharSet.Unicode)] public struct EnabledProfile {
  public uint Type; public ushort Language; public Guid Clsid; public Guid Id; public Guid Category;
  public uint Substitute; public uint Flags;
  [MarshalAs(UnmanagedType.ByValTStr, SizeConst=260)] public string Identifier;
 }
 public struct Foreground {
  public uint Process; public uint Thread; public string Layout;
 }
 public struct ProfileResult {
  public string HResult; public Profile Profile;
 }
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int GetProfile(IntPtr self, uint type, ushort language, ref Guid clsid, ref Guid profile, IntPtr layout, out Profile value);
 public static class Native {
  [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr window, out uint process);
  [DllImport("user32.dll")] static extern IntPtr GetKeyboardLayout(uint thread);
  [DefaultDllImportSearchPaths(DllImportSearchPath.System32)]
  [DllImport("input.dll", CharSet=CharSet.Unicode, ExactSpelling=true)]
  static extern uint EnumEnabledLayoutOrTip(string user, string system, string software, [Out] EnabledProfile[] profiles, uint count);
  public static Foreground GetForeground() {
   uint process; uint thread=GetWindowThreadProcessId(GetForegroundWindow(), out process);
   return new Foreground { Process=process, Thread=thread, Layout="0x"+GetKeyboardLayout(thread).ToInt64().ToString("X") };
  }
  public static EnabledProfile[] GetEnabled() {
   uint count=EnumEnabledLayoutOrTip(null,null,null,null,0);
   if(count>128) throw new InvalidOperationException("Enabled profile count exceeded the diagnostic bound.");
   var profiles=new EnabledProfile[Math.Max(count,1)];
   uint copied=EnumEnabledLayoutOrTip(null,null,null,profiles,(uint)profiles.Length);
   if(copied>profiles.Length) throw new InvalidOperationException("Enabled profile array exceeded its bound.");
   Array.Resize(ref profiles,(int)copied);
   return profiles;
  }
  public static ProfileResult GetOwnProfile() {
   object instance=null; IntPtr unknown=IntPtr.Zero, manager=IntPtr.Zero;
   try {
    instance=Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("33C53A50-F456-4884-B049-85FD643ECFED")));
    unknown=Marshal.GetIUnknownForObject(instance);
    var iid=new Guid("71C6E74C-0F28-11D8-A82A-00065B84435C");
    Marshal.ThrowExceptionForHR(Marshal.QueryInterface(unknown,ref iid,out manager));
    var slot=Marshal.ReadIntPtr(Marshal.ReadIntPtr(manager),5*IntPtr.Size);
    var get=(GetProfile)Marshal.GetDelegateForFunctionPointer(slot,typeof(GetProfile));
    var clsid=new Guid("9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D");
    var id=new Guid("F395A724-DC88-4E75-B3D4-9063A552B8F4");
    Profile profile; int hr=get(manager,1,0x0409,ref clsid,ref id,IntPtr.Zero,out profile);
    return new ProfileResult { HResult="0x"+unchecked((uint)hr).ToString("X8"), Profile=profile };
   } finally {
    if(manager!=IntPtr.Zero) Marshal.Release(manager);
    if(unknown!=IntPtr.Zero) Marshal.Release(unknown);
    if(instance!=null) Marshal.ReleaseComObject(instance);
   }
  }
 }
}
'@
}

function Read-OwnRegistry([Microsoft.Win32.RegistryHive]$Hive, [Microsoft.Win32.RegistryView]$View, [string]$Path) {
  $root = [Microsoft.Win32.RegistryKey]::OpenBaseKey($Hive, $View)
  try {
    $key = $root.OpenSubKey($Path, $false)
    if (-not $key) { return [pscustomobject]@{Hive=[string]$Hive;View=[string]$View;Key=$Path;Present=$false} }
    try {
      $values = [ordered]@{}
      foreach ($name in $key.GetValueNames()) { $values[$name] = $key.GetValue($name) }
      [pscustomobject]@{Hive=[string]$Hive;View=[string]$View;Key=$Path;Present=$true;Values=$values}
    } finally { $key.Dispose() }
  } finally { $root.Dispose() }
}

$clsid = '{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}'
$profile = '{F395A724-DC88-4E75-B3D4-9063A552B8F4}'
$profileKey = "Software\Microsoft\CTF\TIP\$clsid\LanguageProfile\0x00000409\$profile"
$machine = Get-ItemProperty -LiteralPath 'HKLM:\Software\EnglishAssist' -ErrorAction SilentlyContinue
$languages = Get-WinUserLanguageList
$languageList = @(foreach ($language in $languages) {
  [pscustomobject]@{Language=$language.LanguageTag;Tips=@($language.InputMethodTips)}
})
$registry = @(foreach ($view in @([Microsoft.Win32.RegistryView]::Registry64, [Microsoft.Win32.RegistryView]::Registry32)) {
  Read-OwnRegistry LocalMachine $view "Software\Classes\CLSID\$clsid\InprocServer32"
  Read-OwnRegistry LocalMachine $view $profileKey
  Read-OwnRegistry CurrentUser $view $profileKey
})
$sortRoot = 'HKCU:\Software\Microsoft\CTF\SortOrder\AssemblyItem\0x00000409\{34745C63-B2F0-4784-8B67-5E12C8701A31}'
$sortOrder = @(if (Test-Path -LiteralPath $sortRoot) {
  Get-ChildItem -LiteralPath $sortRoot | ForEach-Object {
    $value = Get-ItemProperty -LiteralPath $_.PSPath
    [pscustomobject]@{Index=$_.PSChildName;Clsid=$value.CLSID;Profile=$value.Profile;KeyboardLayout=$value.KeyboardLayout}
  }
})
$modules = @(Get-Process ctfmon,TextInputHost,explorer,notepad,chrome,Code,desktop_host -ErrorAction SilentlyContinue | ForEach-Object {
  $hostProcess = $_
  try {
    $hostProcess.Modules | Where-Object ModuleName -eq 'retype_ime.dll' | ForEach-Object {
      [pscustomobject]@{Pid=$hostProcess.Id;Host=$hostProcess.ProcessName;Module=$_.FileName}
    }
  } catch { } # Protected/inaccessible processes remain unknown, never claimed absent.
})
$signatures = @(if ($machine.ActiveDir) {
  foreach ($relative in @('retype_ime.dll','x86\retype_ime.dll')) {
    $path = Join-Path $machine.ActiveDir $relative
    if (Test-Path -LiteralPath $path) {
      $signature = Get-AuthenticodeSignature -LiteralPath $path
      [pscustomobject]@{Path=$path;Status=[string]$signature.Status}
    }
  }
})
[ordered]@{
  Schema=1;Utc=[DateTime]::UtcNow.ToString('o');ProcessBits=([IntPtr]::Size*8)
  Foreground=[EnglishAssistSystemSnapshot.Native]::GetForeground()
  Note='HKL does not distinguish TSF profiles sharing a language. GetOwnProfile flags describe this diagnostic process, not the foreground thread. A missing LoadedModules entry may mean access was denied.'
  OwnProfile=[EnglishAssistSystemSnapshot.Native]::GetOwnProfile()
  InputDllEnabledProfiles=@([EnglishAssistSystemSnapshot.Native]::GetEnabled())
  UserLanguageList=$languageList;EnglishSortOrder=$sortOrder;Registry=$registry
  LoadedModules=$modules;Signatures=$signatures
} | ConvertTo-Json -Depth 7
