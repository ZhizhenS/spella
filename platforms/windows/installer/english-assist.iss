; English Assist development installer; requires separately built x64/x86 payloads.
; This fork never installs upstream update/sync tasks or changes the default keyboard.
#ifndef RepoRoot
  #error RepoRoot must be the absolute repository directory
#endif
#ifndef MyAppVersion
  #define MyAppVersion "0.1.0"
#endif
#ifndef BaseDir
  #define BaseDir "target\release"
#endif
#ifndef X86Dir
  #define X86Dir "target\i686-pc-windows-msvc\release"
#endif
#ifndef OutDir
  #define OutDir "dist"
#endif
#define MyAppName "English Assist 英文助手"
#define ProductKey "Software\EnglishAssist"
#define TipCLSID "{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}"

[Setup]
AppId={{F981F16C-7472-4F3F-99C5-7C0E47956624}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher=English Assist contributors
VersionInfoProductName={#MyAppName}
VersionInfoDescription=English word completion and spelling assistance
VersionInfoVersion={#MyAppVersion}
DefaultDirName={autopf}\EnglishAssist
DefaultGroupName=English Assist
DisableProgramGroupPage=yes
DisableDirPage=yes
PrivilegesRequired=admin
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
SourceDir={#RepoRoot}
OutputDir={#RepoRoot}\{#OutDir}
OutputBaseFilename=english-assist-{#MyAppVersion}-windows-x64-setup
UninstallDisplayName={#MyAppName}
UninstallDisplayIcon={code:GetPayloadDir}\retype.ico
SetupIconFile={#RepoRoot}\assets\retype.ico
LicenseFile={#RepoRoot}\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupLogging=yes
CloseApplications=no
RestartApplications=no

[Files]
Source: "{#BaseDir}\retype_ime.dll"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion regserver 64bit uninsrestartdelete
Source: "{#X86Dir}\retype_ime.dll"; DestDir: "{code:GetPayloadDir}\x86"; Flags: ignoreversion regserver 32bit uninsrestartdelete
Source: "{#BaseDir}\retype-learning-host.exe"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "{#BaseDir}\retype-ai-host.exe"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "{#BaseDir}\retype-settings-egui.exe"; DestDir: "{code:GetPayloadDir}\settings"; DestName: "retype.exe"; Flags: ignoreversion
Source: "{#BaseDir}\retype-diag.exe"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "assets\retype.ico"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "platforms\windows\installer\user-profile.ps1"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "platforms\windows\installer\cleanup-english-assist.ps1"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "dist\english-dictionaries\english-*.bin"; DestDir: "{code:GetPayloadDir}\dictionaries"; Flags: ignoreversion
Source: "LICENSE"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "NOTICE.txt"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "data\dict\raw\english\LICENSE-*"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion

[Registry]
Root: HKLM64; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "ActiveDir"; ValueData: "{code:GetPayloadDir}"; Flags: uninsdeletevalue
Root: HKLM64; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "Version"; ValueData: "{#MyAppVersion}"; Flags: uninsdeletevalue uninsdeletekeyifempty

[Icons]
Name: "{group}\英文助手设置"; Filename: "{code:GetPayloadDir}\settings\retype.exe"
Name: "{group}\添加到当前用户的英语键盘列表"; Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{code:GetPayloadDir}\user-profile.ps1"""
Name: "{group}\许可与署名"; Filename: "{code:GetPayloadDir}\NOTICE.txt"
Name: "{group}\卸载英文助手"; Filename: "{uninstallexe}"

[Code]
var
  PayloadSuffix, Previous64, Previous32, PreviousPayload: String;
  ProfileAdded, InstallCommitted, RegistrationStarted, DeletePersonalData: Boolean;

function GetPayloadDir(Param: String): String;
begin
  Result := ExpandConstant('{app}\versions\{#MyAppVersion}-') + PayloadSuffix;
end;

function InitializeSetup: Boolean;
begin
  PayloadSuffix := GetDateTimeString('yyyymmddhhnnss', '-', ':') + '-' + IntToStr(Random(1000000));
  RegQueryStringValue(HKLM64, 'Software\Classes\CLSID\{#TipCLSID}\InprocServer32', '', Previous64);
  RegQueryStringValue(HKLM32, 'Software\Classes\CLSID\{#TipCLSID}\InprocServer32', '', Previous32);
  RegQueryStringValue(HKLM64, '{#ProductKey}', 'ActiveDir', PreviousPayload);
  Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var Pending: String;
begin
  Result := '';
  if RegQueryMultiStringValue(HKLM64, 'SYSTEM\CurrentControlSet\Control\Session Manager',
    'PendingFileRenameOperations', Pending) then
    if Pos(Lowercase(ExpandConstant('{app}\')), Lowercase(Pending)) > 0 then
      Result := '英文助手还有文件等待重启处理。请保存工作并重启后再安装。';
end;

procedure CurStepChanged(Step: TSetupStep);
var Code: Integer;
begin
  if Step = ssInstall then RegistrationStarted := True;
  if Step = ssPostInstall then begin
    ProfileAdded := ExecAsOriginalUser(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + GetPayloadDir('') + '\user-profile.ps1"',
      '', SW_HIDE, ewWaitUntilTerminated, Code);
    ProfileAdded := ProfileAdded and (Code = 0);
    Log(Format('English keyboard enrollment: %d', [Code]));
    InstallCommitted := True;
  end;
end;

procedure DeinitializeSetup;
var Code: Integer;
begin
  if RegistrationStarted and not InstallCommitted then begin
    if FileExists(Previous64) then
      Exec(ExpandConstant('{sys}\regsvr32.exe'), '/s "' + Previous64 + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
    if FileExists(Previous32) then
      Exec(ExpandConstant('{syswow64}\regsvr32.exe'), '/s "' + Previous32 + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
    if PreviousPayload <> '' then
      RegWriteStringValue(HKLM64, '{#ProductKey}', 'ActiveDir', PreviousPayload);
  end;
end;

function GetCustomSetupExitCode: Integer;
begin
  Result := 0;
  if not ProfileAdded then Result := 1;
end;

procedure CurPageChanged(PageID: Integer);
begin
  if PageID = wpFinished then begin
    if ProfileAdded then
      WizardForm.FinishedLabel.Caption := '英文助手已加入当前用户的英语键盘列表。' + #13#10 + #13#10 +
        '请用 Win+Space 选择 English Assist 英文助手。英文词库已经随包安装，可离线补词。' + #13#10 +
        '安装不会改变默认键盘。应用仍使用旧版本时，保存工作并重新打开该应用。' + #13#10 + #13#10 +
        '此为开发验证版，真实应用兼容性仍需按验证记录确认。'
    else
      WizardForm.FinishedLabel.Caption := '文件已安装，但英语键盘入口未能自动启用。' + #13#10 +
        '请以日常使用账户运行开始菜单中的“添加到当前用户的英语键盘列表”。' + #13#10 +
        '卸载时也应使用同一账户；文件存在不代表键盘已经生效。';
  end;
end;

function InitializeUninstall: Boolean;
begin
  DeletePersonalData := MsgBox('是否同时删除英文助手的个人词库？' + #13#10 +
    '选“否”会保留个人词库。API 凭据、配置和缓存始终删除。',
    mbConfirmation, MB_YESNO or MB_DEFBUTTON1) = IDYES;
  Result := True;
end;

procedure CurUninstallStepChanged(Step: TUninstallStep);
var Code: Integer; Active, Params: String;
begin
  if Step = usUninstall then begin
    if not RegQueryStringValue(HKLM64, '{#ProductKey}', 'ActiveDir', Active) then
      RaiseException('找不到英文助手安装目录，停止卸载以避免不完整清理。');
    Params := '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + Active +
      '\cleanup-english-assist.ps1" -InstallRoot "' + ExpandConstant('{app}') + '"';
    if DeletePersonalData then Params := Params + ' -DeletePersonalData';
    if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params,
      '', SW_HIDE, ewWaitUntilTerminated, Code) then
      RaiseException('无法启动用户数据清理，停止卸载。');
    if Code <> 0 then
      RaiseException('用户键盘或数据清理失败。请确认正在使用原启用账户并重新尝试，退出码：' + IntToStr(Code));
  end;
end;
