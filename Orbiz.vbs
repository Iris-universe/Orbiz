' Double-clique ce fichier pour lancer Orbiz.
Set fso = CreateObject("Scripting.FileSystemObject")
dir = fso.GetParentFolderName(WScript.ScriptFullName)
exe = dir & "\target\release\orbiz.exe"
If Not fso.FileExists(exe) Then
  MsgBox "Orbiz n'est pas compile." & vbCrLf & "Dans ce dossier : cargo build --release", 48, "Orbiz"
  WScript.Quit
End If
Set sh = CreateObject("WScript.Shell")
sh.CurrentDirectory = dir
sh.Run """" & exe & """", 1, False
