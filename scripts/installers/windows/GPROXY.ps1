param([switch]$OpenConsole)

$ErrorActionPreference = "Stop"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\GPROXY"
$rootDir = Join-Path $env:LOCALAPPDATA "GPROXY"
$dataDir = Join-Path $rootDir "data"
$logDir = Join-Path $rootDir "logs"
$environment = Join-Path $rootDir ".env"
$consoleUrl = "http://127.0.0.1:8787/admin"

New-Item -ItemType Directory -Force -Path $rootDir, $dataDir, $logDir | Out-Null
if (-not (Test-Path $environment)) {
    $bytes = New-Object byte[] 32
    [System.Security.Cryptography.RandomNumberGenerator]::Fill($bytes)
    $secret = [Convert]::ToBase64String($bytes)
    $source = "GPROXY_SECRET_KEY=$secret`n"
    [System.IO.File]::WriteAllText($environment, $source, ([System.Text.UTF8Encoding]::new($false)))
}

function Test-GproxyHealthy {
    try {
        $request = [System.Net.WebRequest]::Create($consoleUrl)
        $request.Timeout = 700
        $response = $request.GetResponse()
        $response.Close()
        return $true
    } catch {
        return $false
    }
}

if (-not (Test-GproxyHealthy)) {
    $process = Start-Process -FilePath (Join-Path $installDir "gproxy.exe") `
        -WorkingDirectory $rootDir `
        -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logDir "gproxy.log") `
        -RedirectStandardError (Join-Path $logDir "gproxy-error.log") -PassThru
    $healthy = $false
    for ($attempt = 0; $attempt -lt 75; $attempt++) {
        Start-Sleep -Milliseconds 200
        if (Test-GproxyHealthy) { $healthy = $true; break }
        if ($process.HasExited) { break }
    }
    if (-not $healthy) {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show(
            "GPROXY did not start. Check the logs in $logDir.", "GPROXY",
            [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Error) | Out-Null
        exit 1
    }
}

if ($OpenConsole) { Start-Process $consoleUrl }
