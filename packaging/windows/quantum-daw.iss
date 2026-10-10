; Instalador de Quantum DAW para Windows 10 y 11 (64 bits). Lo compila Inno Setup:
;   iscc /DVersion=0.7.0 /DBinario=target\release\quantum-daw.exe packaging\windows\quantum-daw.iss
#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Binario
  #define Binario "..\..\target\release\quantum-daw.exe"
#endif

[Setup]
AppId={{6E1C2A5B-3F0D-4E8A-9B7C-51D2A0F4C9E3}
AppName=Quantum DAW
AppVersion={#Version}
AppPublisher=QUANTEX
AppPublisherURL=https://www.quantumdaw.com
AppSupportURL=https://github.com/quantexpy69/quantumdaw
DefaultDirName={autopf}\Quantum DAW
DefaultGroupName=Quantum DAW
LicenseFile=..\..\LICENSE
SetupIconFile=..\..\assets\windows\quantum-daw.ico
UninstallDisplayIcon={app}\quantum-daw.exe
OutputDir=..\..\dist
OutputBaseFilename=quantum-daw-windows-x64-setup
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
; Windows 10 o posterior, solo 64 bits.
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"
Name: "pt"; MessagesFile: "compiler:Languages\BrazilianPortuguese.isl"

[Tasks]
Name: "escritorio"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#Binario}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"

[Icons]
Name: "{group}\Quantum DAW"; Filename: "{app}\quantum-daw.exe"
Name: "{group}\{cm:UninstallProgram,Quantum DAW}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\Quantum DAW"; Filename: "{app}\quantum-daw.exe"; Tasks: escritorio

[Run]
Filename: "{app}\quantum-daw.exe"; Description: "{cm:LaunchProgram,Quantum DAW}"; Flags: nowait postinstall skipifsilent
