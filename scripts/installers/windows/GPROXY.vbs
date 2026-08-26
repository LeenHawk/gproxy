Set shell = CreateObject("WScript.Shell")

Function Quote(value)
  Quote = Chr(34) & value & Chr(34)
End Function

localAppData = shell.ExpandEnvironmentStrings("%LOCALAPPDATA%")
launcher = localAppData & "\Programs\GPROXY\GPROXY.ps1"
command = "powershell.exe -NoLogo -NoProfile -NonInteractive " & _
  "-ExecutionPolicy Bypass -WindowStyle Hidden -File " & Quote(launcher)

If WScript.Arguments.Count > 0 Then
  If LCase(WScript.Arguments(0)) = "open" Then command = command & " -OpenConsole"
End If

shell.Run command, 0, False
