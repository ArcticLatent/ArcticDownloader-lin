#define MyAppName "Arctic Downloader"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
#ifndef SourceExe
  #define SourceExe "target\\release\\arctic-downloader.exe"
#endif

[Setup]
AppId={{2FF1D71E-B6B0-4F2C-8F7B-5E5C1DE5AE38}
AppName={#MyAppName}
AppVersion={#AppVersion}
AppPublisher=Arctic Latent
DefaultDirName={autopf}\Arctic Downloader
DefaultGroupName=Arctic Downloader
DisableProgramGroupPage=yes
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
OutputBaseFilename=ArcticDownloader-setup
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional icons:"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "ArcticDownloader.exe"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Arctic Downloader"; Filename: "{app}\ArcticDownloader.exe"
Name: "{autodesktop}\Arctic Downloader"; Filename: "{app}\ArcticDownloader.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\ArcticDownloader.exe"; Description: "Launch Arctic Downloader"; Flags: nowait postinstall skipifsilent
