$ErrorActionPreference = 'Stop'
Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
namespace EnglishAssistRawProbe {
 [StructLayout(LayoutKind.Sequential)] public struct Profile {
  public uint Type; public ushort Language; public Guid Clsid; public Guid Id; public Guid Category;
  public IntPtr Substitute; public uint Caps; public IntPtr Layout; public uint Flags;
 }
 [StructLayout(LayoutKind.Sequential)] struct Msg {
  public IntPtr Window; public uint Message; public UIntPtr W; public IntPtr L;
  public uint Time; public int X; public int Y; public uint Private;
 }
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int ActivateThread(IntPtr self, out uint id);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int NoArgs(IntPtr self);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int CurrentLanguage(IntPtr self, out ushort language);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int ChangeLanguage(IntPtr self, ushort language);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int Enabled(IntPtr self, ref Guid clsid, ushort language, ref Guid profile, out int enabled);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int Activate(IntPtr self, uint type, ushort language, ref Guid clsid, ref Guid profile, IntPtr layout, uint flags);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int GetProfile(IntPtr self, uint type, ushort language, ref Guid clsid, ref Guid profile, IntPtr layout, out Profile value);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int GetActive(IntPtr self, ref Guid category, out Profile value);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int EnumProfiles(IntPtr self, ushort language, out IntPtr enumerator);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int NextProfile(IntPtr self, uint count, out Profile profile, out uint fetched);
 [UnmanagedFunctionPointer(CallingConvention.StdCall)] delegate int GetSubstitute(IntPtr self, ref Guid clsid, ushort language, ref Guid profile, out IntPtr layout);
 public static class Probe {
  [DllImport("user32.dll")] static extern bool PeekMessageW(out Msg msg, IntPtr window, uint min, uint max, uint remove);
  [DllImport("user32.dll")] static extern bool TranslateMessage(ref Msg msg);
  [DllImport("user32.dll")] static extern IntPtr DispatchMessageW(ref Msg msg);
  [DllImport("user32.dll")] static extern int GetKeyboardLayoutList(int count, [Out] IntPtr[] values);
  static T Slot<T>(IntPtr value, int slot) where T:class {
   return (T)(object)Marshal.GetDelegateForFunctionPointer(Marshal.ReadIntPtr(Marshal.ReadIntPtr(value), slot * IntPtr.Size), typeof(T));
  }
  static string Hr(int value) { return "0x" + unchecked((uint)value).ToString("X8"); }
  static string Describe(Profile p) { return "type="+p.Type+" lang="+p.Language.ToString("X4")+" clsid="+p.Clsid+" profile="+p.Id+" flags="+p.Flags.ToString("X8")+" caps="+p.Caps.ToString("X8"); }
  static IntPtr Query(object obj, string id) {
   var unknown=Marshal.GetIUnknownForObject(obj);
   try { var iid=new Guid(id); IntPtr value; Marshal.ThrowExceptionForHR(Marshal.QueryInterface(unknown,ref iid,out value)); return value; }
   finally { Marshal.Release(unknown); }
  }
  public static string[] Run() {
   var lines=new List<string>();
   object profileObj=null,threadObj=null; IntPtr legacy=IntPtr.Zero,modern=IntPtr.Zero,thread=IntPtr.Zero;
   ushort oldLanguage=0; bool activated=false,restore=false;
   var clsid=new Guid("9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D");
   var profile=new Guid("F395A724-DC88-4E75-B3D4-9063A552B8F4");
   var keyboard=new Guid("34745C63-B2F0-4784-8B67-5E12C8701A31");
   try {
    profileObj=Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("33C53A50-F456-4884-B049-85FD643ECFED")));
    legacy=Query(profileObj,"1F02B6C5-7842-4EE6-8A0B-9A24183A95CA");
    modern=Query(profileObj,"71C6E74C-0F28-11D8-A82A-00065B84435C");
    int enabled; Profile value;
    lines.Add("LegacyEnabled hr="+Hr(Slot<Enabled>(legacy,18)(legacy,ref clsid,0x0409,ref profile,out enabled))+" enabled="+enabled);
    var substitutes=Query(profileObj,"4FD67194-1002-4513-BFF2-C0DDF6258552");
    try { IntPtr layout; int layoutHr=Slot<GetSubstitute>(substitutes,3)(substitutes,ref clsid,0x0409,ref profile,out layout);
      lines.Add("SubstituteLayout hr="+Hr(layoutHr)+" hkl=0x"+layout.ToInt64().ToString("X"));
    } finally { Marshal.Release(substitutes); }
    var layouts=new IntPtr[GetKeyboardLayoutList(0,null)];
    int layoutCount=GetKeyboardLayoutList(layouts.Length,layouts);
    for(int i=0;i<layoutCount;i++) lines.Add("LoadedLayout hkl=0x"+layouts[i].ToInt64().ToString("X"));
    lines.Add("ModernGetProfile hr="+Hr(Slot<GetProfile>(modern,5)(modern,1,0x0409,ref clsid,ref profile,IntPtr.Zero,out value))+" "+Describe(value));
    foreach(ushort language in new ushort[] {0x0409,0x0804}) {
      IntPtr enumeration; int enumHr=Slot<EnumProfiles>(modern,6)(modern,language,out enumeration);
      if(enumHr>=0 && enumeration!=IntPtr.Zero) {
        try { for(int i=0;i<30;i++) {
          uint fetched; int nextHr=Slot<NextProfile>(enumeration,4)(enumeration,1,out value,out fetched);
          if(nextHr<0 || fetched==0) break;
          if(value.Type!=1) continue;
          lines.Add("EnumeratedProfile "+Describe(value));
          var itemClsid=value.Clsid; var itemProfile=value.Id; var itemLanguage=value.Language;
          lines.Add("GetSameEnumeratedProfile hr="+Hr(Slot<GetProfile>(modern,5)(modern,1,itemLanguage,ref itemClsid,ref itemProfile,IntPtr.Zero,out value))+" "+Describe(value));
        }} finally { Marshal.Release(enumeration); }
      }
    }
    int languageHr=Slot<CurrentLanguage>(legacy,13)(legacy,out oldLanguage);
    lines.Add("CurrentLanguage hr="+Hr(languageHr)+" lang="+oldLanguage.ToString("X4"));
    restore=languageHr>=0;
    threadObj=Activator.CreateInstance(Type.GetTypeFromCLSID(new Guid("529A9E6B-6587-4F23-AB9E-9C7D683E3C50")));
    thread=Query(threadObj,"AA80E801-2021-11D2-93E0-0060B067B86E");
    uint client; int threadHr=Slot<ActivateThread>(thread,3)(thread,out client); activated=threadHr>=0;
    lines.Add("ThreadActivate hr="+Hr(threadHr)+" client="+client);
    int beforeHr=Slot<GetActive>(modern,10)(modern,ref keyboard,out value);
    lines.Add("GetActiveBeforeLanguageChange hr="+Hr(beforeHr)+" "+Describe(value));
    if(beforeHr>=0 && value.Type==1) {
      var activeClsid=value.Clsid; var activeProfile=value.Id; var activeLanguage=value.Language;
      lines.Add("GetProfileForExistingActiveService hr="+Hr(Slot<GetProfile>(modern,5)(modern,1,activeLanguage,ref activeClsid,ref activeProfile,IntPtr.Zero,out value))+" "+Describe(value));
    }
    lines.Add("ChangeLanguage hr="+Hr(Slot<ChangeLanguage>(legacy,14)(legacy,0x0409)));
    int hr=Slot<Activate>(modern,3)(modern,1,0x0409,ref clsid,ref profile,IntPtr.Zero,0x10000000);
    lines.Add("ActivateProfile raw_hr="+Hr(hr)+" flags=FORPROCESS (no ENABLEPROFILE)");
    var until=DateTime.UtcNow.AddSeconds(2);
    do {
     Msg msg; while(PeekMessageW(out msg,IntPtr.Zero,0,0,1)) { TranslateMessage(ref msg); DispatchMessageW(ref msg); }
     System.Threading.Thread.Sleep(10);
    } while(DateTime.UtcNow<until);
    lines.Add("GetActive hr="+Hr(Slot<GetActive>(modern,10)(modern,ref keyboard,out value))+" "+Describe(value));
    lines.Add("ModernGetProfileAfter hr="+Hr(Slot<GetProfile>(modern,5)(modern,1,0x0409,ref clsid,ref profile,IntPtr.Zero,out value))+" "+Describe(value));
   } finally {
    if(modern!=IntPtr.Zero) { Slot<Activate>(modern,4)(modern,1,0x0409,ref clsid,ref profile,IntPtr.Zero,0x10000000); }
    if(restore && legacy!=IntPtr.Zero) Slot<ChangeLanguage>(legacy,14)(legacy,oldLanguage);
    if(activated && thread!=IntPtr.Zero) Slot<NoArgs>(thread,4)(thread);
    if(thread!=IntPtr.Zero) Marshal.Release(thread);
    if(modern!=IntPtr.Zero) Marshal.Release(modern);
    if(legacy!=IntPtr.Zero) Marshal.Release(legacy);
    if(threadObj!=null) Marshal.ReleaseComObject(threadObj);
    if(profileObj!=null) Marshal.ReleaseComObject(profileObj);
   }
   return lines.ToArray();
  }
 }
}
'@
[EnglishAssistRawProbe.Probe]::Run()

