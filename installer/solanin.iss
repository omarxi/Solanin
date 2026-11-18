[Setup]
AppName=Solanin
AppVersion=1.0.2
AppPublisher=omarxi
AppPublisherURL=https://github.com/omarxi/Solanin
DefaultDirName={localappdata}\Programs\Solanin
DefaultGroupName=Solanin
PrivilegesRequired=lowest
OutputDir=..\target\installer
OutputBaseFilename=solanin-setup
SetupIconFile=..\assets\icon.ico
UninstallDisplayIcon={app}\solanin.exe
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Tasks]
Name: "desktopicon"; Description: "Create a &Desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Files]
Source: "..\target\release\solanin.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\assets\icon.ico"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\Solanin"; Filename: "{app}\solanin.exe"; IconFilename: "{app}\icon.ico"
Name: "{group}\Uninstall Solanin"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Solanin"; Filename: "{app}\solanin.exe"; IconFilename: "{app}\icon.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\solanin.exe"; Description: "Launch Solanin"; Flags: nowait postinstall skipifsilent
