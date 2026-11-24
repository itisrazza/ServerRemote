; The Windows installer for GitHub releases (the Microsoft Store gets an
; MSIX instead). Installs for the current user without asking for admin
; rights; the user can choose an all-users install instead.
;
; Build with Inno Setup 6:
;   iscc /DAppVersion=0.1.0 /DArch=<x64|arm64> /DSourceDir=<folder with server-remote.exe>
;        /DOutputName=<installer name without .exe> /O<output dir>
;        packaging\windows\ServerRemote.iss

#ifndef AppVersion
  #error Pass /DAppVersion=<x.y.z>
#endif
#ifndef SourceDir
  #error Pass /DSourceDir=<folder holding server-remote.exe and the licences>
#endif
#ifndef Arch
  #error Pass /DArch=x64 or /DArch=arm64
#endif
#if Arch == "arm64"
  #define Architectures "arm64"
#elif Arch == "x64"
  ; x64compatible also lets it install on ARM64 Windows under emulation.
  #define Architectures "x64compatible"
#else
  #error Arch must be x64 or arm64
#endif
#ifndef OutputName
  #define OutputName "server-remote-" + AppVersion + "-windows-" + Arch + "-setup"
#endif

[Setup]
; Never change AppId: Windows matches upgrades and uninstalls by it.
AppId={{E327B21B-F65A-4645-955E-10F50FCF3FE5}
AppName=ServerRemote
AppVersion={#AppVersion}
AppPublisher=Rareș Nistor
AppPublisherURL=https://serverremote.razza.io/
AppSupportURL=https://github.com/itisrazza/ServerRemote/issues
AppUpdatesURL=https://github.com/itisrazza/ServerRemote/releases
VersionInfoVersion={#AppVersion}
DefaultDirName={autopf}\ServerRemote
DefaultGroupName=ServerRemote
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
ArchitecturesAllowed={#Architectures}
ArchitecturesInstallIn64BitMode={#Architectures}
MinVersion=10.0
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\server-remote.exe
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
OutputBaseFilename={#OutputName}
; A running copy would keep the old .exe in place.
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\server-remote.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-MIT"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE-APACHE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\OFL-NunitoSans.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\ServerRemote"; Filename: "{app}\server-remote.exe"
Name: "{autodesktop}\ServerRemote"; Filename: "{app}\server-remote.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\server-remote.exe"; Description: "{cm:LaunchProgram,ServerRemote}"; Flags: nowait postinstall skipifsilent
