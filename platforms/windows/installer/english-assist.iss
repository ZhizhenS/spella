; English Assist development installer; requires separately built x64/x86 payloads.
; This fork never installs upstream update/sync tasks or changes the default keyboard.
#ifndef RepoRoot
  #error RepoRoot must be the absolute repository directory
#endif
#ifndef MyAppVersion
  #error MyAppVersion must come from the verified frozen payload
#endif
#ifndef PayloadRoot
  #error PayloadRoot must be an absolute verified production freeze; use repack-english-assist.ps1
#endif
#ifndef OutDir
  #define OutDir "dist"
#endif
#define MyAppName "English Assist 英文助手"
#define ProductKey "Software\EnglishAssist"
#define ProductId "{F981F16C-7472-4F3F-99C5-7C0E47956624}"
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
SourceDir={#PayloadRoot}
OutputDir={#RepoRoot}\{#OutDir}
OutputBaseFilename=english-assist-{#MyAppVersion}-windows-x64-setup
UninstallDisplayName={#MyAppName}
UninstallDisplayIcon={code:GetPayloadDir}\retype.ico
SetupIconFile={#PayloadRoot}\retype.ico
LicenseFile={#PayloadRoot}\LICENSE
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupLogging=yes
CloseApplications=no
RestartApplications=no

[Files]
Source: "retype_ime.dll"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion regserver 64bit uninsrestartdelete
Source: "x86\retype_ime.dll"; DestDir: "{code:GetPayloadDir}\x86"; Flags: ignoreversion regserver 32bit uninsrestartdelete
Source: "retype-learning-host.exe"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "retype-ai-host.exe"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "settings\retype.exe"; DestDir: "{code:GetPayloadDir}\settings"; DestName: "retype.exe"; Flags: ignoreversion
Source: "retype-diag.exe"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "retype.ico"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "user-profile.ps1"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "cleanup-english-assist.ps1"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "install-state.ps1"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "manual-words-access.ps1"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "powershell-environment.ps1"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "dictionaries\english-e1098fbaa665d52788f6ed1fde2a00eb8145e459c41583f5e59f4678db7619bc.bin"; DestDir: "{code:GetPayloadDir}\dictionaries"; Flags: ignoreversion
Source: "dictionaries\english-lexicon-v2.bin"; DestDir: "{code:GetPayloadDir}\dictionaries"; Flags: ignoreversion
Source: "dictionaries\english-lexicon-v2.json"; DestDir: "{code:GetPayloadDir}\dictionaries"; Flags: ignoreversion
Source: "LICENSE"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "NOTICE.txt"; DestDir: "{code:GetPayloadDir}"; DestName: "NOTICE.txt"; Flags: ignoreversion
Source: "LICENSE-Google-Ngram"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "LICENSE-SCOWL"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "LICENSE-SymSpell"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "LICENSE-ECDICT"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "LICENSE-CC-CEDICT"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "LICENSE-Wanxiang"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "NOTICE-English-Lexicon.txt"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion
Source: "english-lexicon-sources.json"; DestDir: "{code:GetPayloadDir}"; DestName: "english-lexicon-sources.json"; Flags: ignoreversion

Source: "build.json"; DestDir: "{code:GetPayloadDir}"; Flags: ignoreversion

[Registry]
; Logged first so it runs last during reverse-order uninstall.
Root: HKLM64; Subkey: "{#ProductKey}"; Flags: uninsdeletekeyifempty
Root: HKLM64; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "ActiveDir"; ValueData: "{code:GetPayloadDir}"; Flags: uninsdeletevalue
Root: HKLM64; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "Version"; ValueData: "{#MyAppVersion}"; Flags: uninsdeletevalue
Root: HKLM64; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "ProductId"; ValueData: "{{F981F16C-7472-4F3F-99C5-7C0E47956624}"; Flags: uninsdeletevalue
Root: HKLM64; Subkey: "{#ProductKey}"; ValueType: string; ValueName: "TipClsid"; ValueData: "{{9257DCE8-EC1B-4D4A-8EA0-8795A9FD881D}"; Flags: uninsdeletevalue
Root: HKLM64; Subkey: "{#ProductKey}"; ValueType: dword; ValueName: "FreshInstall"; ValueData: "{code:GetFreshInstall}"; Flags: uninsdeletevalue

[Icons]
Name: "{group}\英文助手设置"; Filename: "{code:GetPayloadDir}\settings\retype.exe"
Name: "{group}\添加到当前用户的英语键盘列表"; Filename: "{sys}\WindowsPowerShell\v1.0\powershell.exe"; Parameters: "-NoProfile -ExecutionPolicy Bypass -File ""{code:GetPayloadDir}\user-profile.ps1"""
Name: "{group}\许可与署名"; Filename: "{code:GetPayloadDir}\NOTICE.txt"
Name: "{group}\卸载英文助手"; Filename: "{uninstallexe}"

[Code]
var
  PayloadSuffix, Previous64, Previous32, PreviousPayload: String;
  PreviousVersion, PreviousProductId, PreviousTipClsid: String;
  PreviousFreshInstall: Cardinal;
  PreviousFreshExists: Boolean;
  ProfileAdded, InstallCommitted, RegistrationStarted, DeletePersonalData: Boolean;

function GetPayloadDir(Param: String): String;
begin
  Result := ExpandConstant('{app}\versions\{#MyAppVersion}-') + PayloadSuffix;
end;

function GetFreshInstall(Param: String): String;
begin
  Result := '0';
  if (PreviousPayload = '') and (Previous64 = '') and (Previous32 = '') then
    Result := '1';
end;

function InitializeSetup: Boolean;
begin
  PayloadSuffix := GetDateTimeString('yyyymmddhhnnss', '-', ':') + '-' + IntToStr(Random(1000000));
  RegQueryStringValue(HKLM64, 'Software\Classes\CLSID\{#TipCLSID}\InprocServer32', '', Previous64);
  RegQueryStringValue(HKLM32, 'Software\Classes\CLSID\{#TipCLSID}\InprocServer32', '', Previous32);
  RegQueryStringValue(HKLM64, '{#ProductKey}', 'ActiveDir', PreviousPayload);
  RegQueryStringValue(HKLM64, '{#ProductKey}', 'Version', PreviousVersion);
  RegQueryStringValue(HKLM64, '{#ProductKey}', 'ProductId', PreviousProductId);
  RegQueryStringValue(HKLM64, '{#ProductKey}', 'TipClsid', PreviousTipClsid);
  PreviousFreshExists := RegQueryDWordValue(HKLM64, '{#ProductKey}', 'FreshInstall', PreviousFreshInstall);
  Result := True;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var InstallPath, ExpectedPath, Params: String; Code: Integer;
begin
  Result := '';
  InstallPath := RemoveBackslash(ExpandFileName(ExpandConstant('{app}')));
  ExpectedPath := RemoveBackslash(ExpandFileName(ExpandConstant('{commonpf64}\EnglishAssist')));
  if CompareText(InstallPath, ExpectedPath) <> 0 then begin
    Result := '英文助手必须安装到独立目录：' + ExpectedPath + '。请移除 /DIR 覆盖参数后重试。';
    Exit;
  end;
  // The existing product may have old, loaded TIP DLLs queued only for deletion.
  // A fresh unique payload can coexist with those files. All other operations
  // touching this product remain blocked; the guard never edits the OS queue.
  ExtractTemporaryFile('powershell-environment.ps1');
  ExtractTemporaryFile('install-state.ps1');
  Params := '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' +
    ExpandConstant('{tmp}\install-state.ps1') + '" -CheckPendingRenames -PendingPolicyInstallRoot "' +
    InstallPath + '" -PendingPolicyNewPayload "' + GetPayloadDir('') + '"';
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'), Params,
      '', SW_HIDE, ewWaitUntilTerminated, Code) then
    Result := '无法验证英文助手的待重启文件队列，安装尚未开始。'
  else if Code <> 0 then
    Result := '待重启队列包含无法安全忽略的英文助手操作，或安装路径存在重解析。请重启后重试。';
end;

procedure CurStepChanged(Step: TSetupStep);
var Code: Integer; ActiveReport: String;
begin
  if Step = ssInstall then RegistrationStarted := True;
  if Step = ssPostInstall then begin
    // Inno has already committed machine installation at ssPostInstall.
    // A user-enrollment failure is recoverable; it must not partially restore
    // only COM/ActiveDir while leaving the new uninstall log in place.
    InstallCommitted := True;
    ProfileAdded := False;
    Code := -1;
    try
      ProfileAdded := ExecAsOriginalUser(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
        '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' + GetPayloadDir('') + '\user-profile.ps1"',
        '', SW_HIDE, ewWaitUntilTerminated, Code);
      ProfileAdded := ProfileAdded and (Code = 0);
    except
      Log('English keyboard enrollment exception: ' + GetExceptionMessage);
    end;
    Log(Format('English keyboard enrollment: %d', [Code]));
    if not RegQueryStringValue(HKLM64, '{#ProductKey}', 'ActiveDir', ActiveReport) then
      ActiveReport := '<missing>';
    Log('English Assist install report: SourceVersion={#MyAppVersion}; ActualActiveDir=' + ActiveReport);
  end;
end;

procedure RestoreMachineString(Name, Value: String);
begin
  if Value <> '' then begin
    if not RegWriteStringValue(HKLM64, '{#ProductKey}', Name, Value) then
      Log('Could not restore previous installation value: ' + Name);
  end else
    RegDeleteValue(HKLM64, '{#ProductKey}', Name);
end;

procedure DeinitializeSetup;
var Code: Integer;
begin
  if RegistrationStarted and not InstallCommitted then begin
    if FileExists(Previous64) then
      Exec(ExpandConstant('{sys}\regsvr32.exe'), '/s "' + Previous64 + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
    if FileExists(Previous32) then
      Exec(ExpandConstant('{syswow64}\regsvr32.exe'), '/s "' + Previous32 + '"', '', SW_HIDE, ewWaitUntilTerminated, Code);
    RestoreMachineString('ActiveDir', PreviousPayload);
    RestoreMachineString('Version', PreviousVersion);
    RestoreMachineString('ProductId', PreviousProductId);
    RestoreMachineString('TipClsid', PreviousTipClsid);
    if PreviousFreshExists then
      RegWriteDWordValue(HKLM64, '{#ProductKey}', 'FreshInstall', PreviousFreshInstall)
    else
      RegDeleteValue(HKLM64, '{#ProductKey}', 'FreshInstall');
    RegDeleteKeyIfEmpty(HKLM64, '{#ProductKey}');
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
        '升级后，请保存工作并重新打开要使用英文助手的应用，以加载新版。' + #13#10 + #13#10 +
        '此为开发验证版，真实应用兼容性仍需按验证记录确认。'
    else
      WizardForm.FinishedLabel.Caption := '文件已安装，但英语键盘入口未能自动启用。' + #13#10 +
        '请以日常使用账户运行开始菜单中的“添加到当前用户的英语键盘列表”。' + #13#10 +
        '卸载时也应使用同一账户；文件存在不代表键盘已经生效。';
  end;
end;

function InitializeUninstall: Boolean;
begin
  if UninstallSilent then
    DeletePersonalData := ExpandConstant('{param:DELETEPERSONALDATA|0}') = '1'
  else
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
