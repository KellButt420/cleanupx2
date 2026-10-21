#ifndef AppVersion
  #error AppVersion must be supplied by package-desktop.ps1
#endif
#ifndef SourceDir
  #error SourceDir must be supplied by package-desktop.ps1
#endif
#ifndef OutputDir
  #error OutputDir must be supplied by package-desktop.ps1
#endif

[Setup]
AppId={{DB9EB688-8CC9-4CA8-908E-5658412F8E25}
AppName=cleanup
AppVersion={#AppVersion}
AppPublisher=(wired-in) community
AppPublisherURL=https://cleanup-wiredin.vercel.app
AppSupportURL=https://discord.gg/5kRtCcNWU6
AppUpdatesURL=https://github.com/wiredincorp/cleanup/releases
DefaultDirName={localappdata}\Programs\cleanup
DefaultGroupName=cleanup
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.22000
UninstallDisplayIcon={app}\Cleanup.exe
UninstallDisplayName=cleanup
OutputDir={#OutputDir}
OutputBaseFilename=cleanup-{#AppVersion}-windows-x64-setup
SetupIconFile={#SourceDir}\Assets\Cleanup.ico
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
CloseApplications=no
RestartApplications=no
AppMutex=Local\Cleanup.Desktop.MainWindow
UsePreviousAppDir=yes

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Shortcuts:"; Flags: unchecked

[Icons]
Name: "{autoprograms}\cleanup"; Filename: "{app}\Cleanup.exe"; WorkingDir: "{app}"
Name: "{autodesktop}\cleanup"; Filename: "{app}\Cleanup.exe"; WorkingDir: "{app}"; Tasks: desktopicon

[Run]
Filename: "{app}\Cleanup.exe"; Description: "Open cleanup"; Flags: nowait postinstall skipifsilent runasoriginaluser

; Deliberately do not delete %LOCALAPPDATA%\Cleanup on uninstall.
