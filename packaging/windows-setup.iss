#if Ver != EncodeVer(6, 4, 1)
  #error Build with Inno Setup 6.4.1
#endif

[Setup]
AppId=OkColorPicker
AppName=OkColorPicker
AppVersion={#ProductVersion}
AppPublisher=sheetau
DefaultDirName={localappdata}\Programs\OkColorPicker
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DisableProgramGroupPage=yes
DefaultGroupName=OkColorPicker
AllowNoIcons=yes
OutputBaseFilename=OkColorPicker-{#ProductVersion}-windows-x64-setup
SetupIconFile=..\crates\okcolor-app\assets\okcolorpicker.ico
UninstallDisplayIcon={app}\OkColorPicker.exe
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
RestartApplications=no
VersionInfoVersion={#ProductVersion}.0
VersionInfoDescription=OkColorPicker Setup

[Files]
Source: "{#PayloadDir}\*"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\OkColorPicker"; Filename: "{app}\OkColorPicker.exe"

[UninstallDelete]
Type: files; Name: "{app}\.okcolor-update\update.lock"
Type: files; Name: "{app}\.okcolor-update\setup-pending.json"
Type: files; Name: "{app}\.okcolor-update\setup-update.exe"
Type: dirifempty; Name: "{app}\.okcolor-update"

[Code]
const
  RuntimeKey = 'SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64';
  RuntimeUrl = 'https://download.visualstudio.microsoft.com/download/pr/ebdab8e5-1d7b-4d9f-a11b-cbb1720c3b12/843068991DAAA1F73AD9F6239BCE4D0F6A07A51F18C37EA2A867E9BECA71295C/VC_redist.x64.exe';
  RuntimeHash = '843068991daaa1f73ad9f6239bce4d0f6a07a51f18c37ea2a867e9beca71295c';

var
  DownloadPage: TDownloadWizardPage;
  RuntimeRestart: Boolean;

function RuntimeRegistered: Boolean;
var
  Installed: Cardinal;
  Version: String;
  Packed: Int64;
begin
  Result := False;
  if RegQueryDWordValue(HKLM64, RuntimeKey, 'Installed', Installed)
    and (Installed = 1) and RegQueryStringValue(HKLM64, RuntimeKey, 'Version', Version) then
  begin
    if Copy(Version, 1, 1) = 'v' then Delete(Version, 1, 1);
    if StrToVersion(Version, Packed) then
      Result := ComparePackedVersion(Packed, PackVersionComponents(14, 51, 36247, 0)) >= 0;
  end;
end;

function RuntimeReady: Boolean;
begin
  Result := RuntimeRegistered and FileExists(ExpandConstant('{sysnative}\msvcp140.dll'))
    and FileExists(ExpandConstant('{sysnative}\vcruntime140.dll'))
    and FileExists(ExpandConstant('{sysnative}\vcruntime140_1.dll'));
end;

procedure InitializeWizard;
begin
  DownloadPage := CreateDownloadPage('Microsoft Visual C++ Runtime',
    'Setup installs the required x64 runtime from Microsoft.', nil);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ExitCode: Integer;
  Action: String;
begin
  Result := '';
  if RuntimeReady then Exit;
  if RuntimeRegistered then Action := '/repair' else Action := '/install';
  DownloadPage.Clear;
  DownloadPage.Add(RuntimeUrl, 'vc_redist.x64.exe', RuntimeHash);
  DownloadPage.Show;
  try
    try
      DownloadPage.Download;
      DownloadPage.SetText('Installing the Microsoft Visual C++ x64 runtime...', '');
      if not ShellExec('runas', ExpandConstant('{tmp}\vc_redist.x64.exe'), Action + ' /quiet /norestart',
        '', SW_HIDE, ewWaitUntilTerminated, ExitCode) then
        Result := SysErrorMessage(ExitCode)
      else if (ExitCode <> 0) and (ExitCode <> 3010) then
        Result := Format('Microsoft runtime installation returned %d.', [ExitCode])
      else if not RuntimeReady then
      begin
        NeedsRestart := ExitCode = 3010;
        Result := 'The required Microsoft runtime is still unavailable.';
      end
      else
        RuntimeRestart := ExitCode = 3010;
    except
      Result := GetExceptionMessage;
    end;
  finally
    DownloadPage.Hide;
  end;
end;

function NeedRestart: Boolean;
begin
  Result := RuntimeRestart;
end;
