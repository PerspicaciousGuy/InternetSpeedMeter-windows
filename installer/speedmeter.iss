; Built by CI: ISCC /DAppVersion=<x.y.z> installer\speedmeter.iss
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6F0E7B1C-3D52-4C8E-9A57-2B8F1D4E0A13}
AppName=Internet Speed Meter
AppVersion={#AppVersion}
AppPublisher=PerspicaciousGuy
DefaultDirName={autopf}\Internet Speed Meter
DefaultGroupName=Internet Speed Meter
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\dist
OutputBaseFilename=InternetSpeedMeter-Setup-{#AppVersion}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=Internet Speed Meter

[Tasks]
Name: "autostart"; Description: "Start Internet Speed Meter when I sign in to Windows"

[Files]
Source: "..\target\release\speedmeter.exe"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Internet Speed Meter"; Filename: "{app}\speedmeter.exe"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "InternetSpeedMeter"; ValueData: """{app}\speedmeter.exe"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
Filename: "{app}\speedmeter.exe"; Description: "Launch Internet Speed Meter"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/F /IM speedmeter.exe"; Flags: runhidden; RunOnceId: "StopSpeedMeter"

[Code]
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  // Stop a running copy so the exe can be replaced on upgrade.
  Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM speedmeter.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Result := '';
end;
