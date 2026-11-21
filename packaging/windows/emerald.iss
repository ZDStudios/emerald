; Emerald installer (Inno Setup 6). Per-user install: no administrator rights needed.
; Built by .github/workflows/release.yml:
;   ISCC /DAppVersion=0.1.0 /DSourceDir=..\..\dist\Emerald packaging\windows\emerald.iss

#ifndef AppVersion
  #define AppVersion "0.1.0"
#endif
#ifndef SourceDir
  #define SourceDir "..\..\dist\Emerald"
#endif

[Setup]
AppId={{7B1C5E2A-6D41-4E0F-9C3B-5A8E2F1D0E77}
AppName=Emerald
AppVersion={#AppVersion}
AppPublisher=Emerald
AppVerName=Emerald {#AppVersion}
DefaultDirName={localappdata}\Programs\Emerald
DefaultGroupName=Emerald
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir=..\..\dist
OutputBaseFilename=Emerald-{#AppVersion}-setup-x64
SetupIconFile=emerald.ico
UninstallDisplayIcon={app}\emerald.exe
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
MinVersion=10.0.17763

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked
Name: "registerbrowser"; Description: "Make Emerald available as a default-browser choice in Windows Settings"; GroupDescription: "Integration:"

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Emerald"; Filename: "{app}\emerald.exe"
Name: "{autodesktop}\Emerald"; Filename: "{app}\emerald.exe"; Tasks: desktopicon

[Registry]
; Register as a browser so it shows up in Settings > Apps > Default apps (the user still chooses).
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald"; ValueType: string; ValueData: "Emerald"; Flags: uninsdeletekey; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\DefaultIcon"; ValueType: string; ValueData: "{app}\emerald.exe,0"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\shell\open\command"; ValueType: string; ValueData: """{app}\emerald.exe"""; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "Emerald"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "A calm, minimal, customizable browser"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\Capabilities\URLAssociations"; ValueType: string; ValueName: "http"; ValueData: "EmeraldHTML"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\Capabilities\URLAssociations"; ValueType: string; ValueName: "https"; ValueData: "EmeraldHTML"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\Capabilities\FileAssociations"; ValueType: string; ValueName: ".html"; ValueData: "EmeraldHTML"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Clients\StartMenuInternet\Emerald\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pdf"; ValueData: "EmeraldHTML"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "Emerald"; ValueData: "Software\Clients\StartMenuInternet\Emerald\Capabilities"; Flags: uninsdeletevalue; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Classes\EmeraldHTML"; ValueType: string; ValueData: "Emerald HTML Document"; Flags: uninsdeletekey; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Classes\EmeraldHTML\DefaultIcon"; ValueType: string; ValueData: "{app}\emerald.exe,0"; Tasks: registerbrowser
Root: HKCU; Subkey: "Software\Classes\EmeraldHTML\shell\open\command"; ValueType: string; ValueData: """{app}\emerald.exe"" ""%1"""; Tasks: registerbrowser

[Run]
Filename: "{app}\emerald.exe"; Description: "Launch Emerald"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Browsing data lives in %LOCALAPPDATA%\Emerald and is kept on uninstall unless removed manually.
Type: filesandordirs; Name: "{app}"
