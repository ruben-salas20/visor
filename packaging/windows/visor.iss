; Installer of visor for Windows (Inno Setup 6), built by .github/workflows/release.yml:
;   iscc /DVersion=0.3.0 /DNumVersion=0.3.0 /DBin=<folder with the .exe files>
;        /DWebView2=<MicrosoftEdgeWebview2Setup.exe> /DOutput=<folder> /DOutputName=<name> visor.iss
;
; Per user and without admin rights, so that visor can replace its own visor.exe
; when it updates (src/update.rs).

#ifndef Version
  #error Pass /DVersion=<version>
#endif

[Setup]
AppId={{4CED6D7E-C0DA-4E02-83B1-206BC4C2A065}
AppName=visor
AppVersion={#Version}
AppVerName=visor {#Version}
AppPublisher=Ruben Salas
AppPublisherURL=https://github.com/ruben-salas20/visor
AppSupportURL=https://github.com/ruben-salas20/visor/issues
VersionInfoVersion={#NumVersion}
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\visor
; Elsewhere, such as Program Files, visor could not update itself.
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
ChangesAssociations=yes
UninstallDisplayIcon={app}\visorw.exe
UninstallDisplayName=visor
WizardStyle=modern
ShowLanguageDialog=no
OutputDir={#Output}
OutputBaseFilename={#OutputName}
Compression=lzma2/max
SolidCompression=yes

[Languages]
Name: "es"; MessagesFile: "compiler:Languages\Spanish.isl"

[CustomMessages]
es.InstallingWebView2=Instalando Microsoft Edge WebView2, que visor necesita para mostrar las páginas…

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#Bin}\visor.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Bin}\visorw.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#WebView2}"; DestDir: "{tmp}"; DestName: "MicrosoftEdgeWebview2Setup.exe"; Flags: deleteafterinstall; Check: NeedsWebView2

[Icons]
Name: "{autoprograms}\visor"; Filename: "{app}\visorw.exe"
Name: "{autodesktop}\visor"; Filename: "{app}\visorw.exe"; Tasks: desktopicon

[Registry]
; «Abrir con» for .md and .html: listed as an option (OpenWithProgids), never the
; default program. visor.exe would take a bare path as a command, hence «open».
Root: HKA; Subkey: "Software\Classes\visor.md"; ValueType: string; ValueName: ""; ValueData: "Nota de Markdown"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\visor.md\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\visorw.exe"" open --loose ""%1"""
Root: HKA; Subkey: "Software\Classes\visor.html"; ValueType: string; ValueName: ""; ValueData: "Página HTML"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\visor.html\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\visorw.exe"" open --loose ""%1"""
Root: HKA; Subkey: "Software\Classes\.md\OpenWithProgids"; ValueType: none; ValueName: "visor.md"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.markdown\OpenWithProgids"; ValueType: none; ValueName: "visor.md"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.html\OpenWithProgids"; ValueType: none; ValueName: "visor.html"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.htm\OpenWithProgids"; ValueType: none; ValueName: "visor.html"; Flags: uninsdeletevalue
; The name «Abrir con» shows, instead of «visorw.exe».
Root: HKA; Subkey: "Software\Classes\Applications\visorw.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "visor"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\visorw.exe\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\visorw.exe"" open --loose ""%1"""
Root: HKA; Subkey: "Software\Classes\Applications\visorw.exe\SupportedTypes"; ValueType: string; ValueName: ".md"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\visorw.exe\SupportedTypes"; ValueType: string; ValueName: ".markdown"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\visorw.exe\SupportedTypes"; ValueType: string; ValueName: ".html"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\visorw.exe\SupportedTypes"; ValueType: string; ValueName: ".htm"; ValueData: ""

[Run]
Filename: "{tmp}\MicrosoftEdgeWebview2Setup.exe"; Parameters: "/silent /install"; StatusMsg: "{cm:InstallingWebView2}"; Flags: waituntilterminated; Check: NeedsWebView2
Filename: "{app}\visorw.exe"; Description: "{cm:LaunchProgram,visor}"; Flags: postinstall nowait skipifsilent

[UninstallDelete]
; What an update leaves beside visor.exe (src/update.rs).
Type: files; Name: "{app}\visor.exe.old"
Type: files; Name: "{app}\.visor-nuevo-*"

[Code]
const
  WebView2Key = 'Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function Installed(Root: Integer; Key: String): Boolean;
var
  Version: String;
begin
  Result := RegQueryStringValue(Root, Key, 'pv', Version) and (Version <> '') and (Version <> '0.0.0.0');
end;

// Where Microsoft says to look: per machine (32-bit view) or per user.
function NeedsWebView2: Boolean;
begin
  Result := not (Installed(HKLM32, 'SOFTWARE\' + WebView2Key) or Installed(HKCU, 'Software\' + WebView2Key));
end;
