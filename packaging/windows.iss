; ISCC /DAppVersion=0.1.0 /DSourceDir=... /DReleaseDir=... /DOutputName=... windows.iss
[Setup]
AppId={{A0711529-21D9-41B5-9EC5-86C32D137A87}
AppName=Rivet
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\Rivet
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
OutputDir={#ReleaseDir}
OutputBaseFilename={#OutputName}
Compression=lzma2
SolidCompression=yes
UninstallDisplayIcon={app}\bin\rivet-desktop.exe

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{userprograms}\Rivet"; Filename: "{app}\bin\rivet-desktop.exe"

[Run]
Filename: "{app}\bin\rivet-desktop.exe"; Description: "Launch Rivet and configure the default browser"; Flags: nowait postinstall skipifsilent

; User downloads/history are preserved. Remove only this app's per-user native-host registration.
[Registry]
Root: HKCU; Subkey: "Software\Google\Chrome\NativeMessagingHosts\org.rivet.downloads"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Microsoft\Edge\NativeMessagingHosts\org.rivet.downloads"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Chromium\NativeMessagingHosts\org.rivet.downloads"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Mozilla\NativeMessagingHosts\org.rivet.downloads"; Flags: uninsdeletekey
