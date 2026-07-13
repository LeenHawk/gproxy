Set shell = CreateObject("WScript.Shell")
Function Quote(value)
  Quote = Chr(34) & value & Chr(34)
End Function

localAppData = shell.ExpandEnvironmentStrings("%LOCALAPPDATA%")
exe = localAppData & "\Programs\GPROXY\gproxy.exe"
dataDir = localAppData & "\GPROXY\data"
logDir = localAppData & "\GPROXY\logs"
logFile = logDir & "\gproxy.log"
Set fileSystem = CreateObject("Scripting.FileSystemObject")
If Not fileSystem.FolderExists(localAppData & "\GPROXY") Then
  fileSystem.CreateFolder(localAppData & "\GPROXY")
End If
If Not fileSystem.FolderExists(dataDir) Then fileSystem.CreateFolder(dataDir)
If Not fileSystem.FolderExists(logDir) Then fileSystem.CreateFolder(logDir)
server = Quote(exe) & " --data-dir " & Quote(dataDir)
command = Quote(shell.ExpandEnvironmentStrings("%COMSPEC%")) & _
  " /d /c " & Quote(server & " >> " & Quote(logFile) & " 2>&1")
shell.CurrentDirectory = localAppData & "\GPROXY"
shell.Run command, 0, False
If WScript.Arguments.Count > 0 Then
  If WScript.Arguments(0) = "open" Then
    WScript.Sleep 1200
    shell.Run "http://127.0.0.1:8787/console", 1, False
  End If
End If
