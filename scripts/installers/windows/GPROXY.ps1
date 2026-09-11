param([switch]$OpenConsole, [switch]$Packaged)

$ErrorActionPreference = "Stop"
$installDir = $PSScriptRoot
$rootDir = if ($Packaged) {
    Join-Path ([Windows.Storage.ApplicationData, Windows.Storage, ContentType=WindowsRuntime]::Current.LocalFolder.Path) "GPROXY"
} else {
    Join-Path $env:LOCALAPPDATA "GPROXY"
}
$dataDir = Join-Path $rootDir "data"
$logDir = Join-Path $rootDir "logs"
$environment = Join-Path $rootDir ".env"
$consoleUrl = "http://127.0.0.1:8787/admin"

New-Item -ItemType Directory -Force -Path $rootDir, $dataDir, $logDir | Out-Null
if (-not (Test-Path $environment)) {
    $bytes = New-Object byte[] 32
    $random = [System.Security.Cryptography.RandomNumberGenerator]::Create()
    try { $random.GetBytes($bytes) } finally { $random.Dispose() }
    $secret = [Convert]::ToBase64String($bytes)
    $source = "GPROXY_MASTER_KEY=$secret`n"
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

function Show-SetupDialog {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing

    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Set up GPROXY"
    $form.ClientSize = New-Object System.Drawing.Size(500, 240)
    $form.StartPosition = "CenterScreen"
    $form.FormBorderStyle = "FixedDialog"
    $form.MaximizeBox = $false
    $form.MinimizeBox = $false

    $intro = New-Object System.Windows.Forms.Label
    $intro.Location = New-Object System.Drawing.Point(20, 18)
    $intro.Size = New-Object System.Drawing.Size(460, 42)
    $intro.Text = "Choose the administrator credentials used to sign in to Console. GPROXY stores the password hash; this launcher does not save the plaintext."

    $userLabel = New-Object System.Windows.Forms.Label
    $userLabel.Location = New-Object System.Drawing.Point(20, 72)
    $userLabel.Size = New-Object System.Drawing.Size(130, 20)
    $userLabel.Text = "Admin username"

    $userInput = New-Object System.Windows.Forms.TextBox
    $userInput.Location = New-Object System.Drawing.Point(160, 69)
    $userInput.Size = New-Object System.Drawing.Size(320, 24)
    $userInput.Text = "admin"

    $passwordLabel = New-Object System.Windows.Forms.Label
    $passwordLabel.Location = New-Object System.Drawing.Point(20, 108)
    $passwordLabel.Size = New-Object System.Drawing.Size(130, 20)
    $passwordLabel.Text = "Admin password"

    $passwordInput = New-Object System.Windows.Forms.TextBox
    $passwordInput.Location = New-Object System.Drawing.Point(160, 105)
    $passwordInput.Size = New-Object System.Drawing.Size(320, 24)
    $passwordInput.UseSystemPasswordChar = $true

    $autostartInput = New-Object System.Windows.Forms.CheckBox
    $autostartInput.Location = New-Object System.Drawing.Point(20, 147)
    $autostartInput.Size = New-Object System.Drawing.Size(400, 24)
    $autostartInput.Text = "Start GPROXY automatically when I sign in"
    $autostartInput.Checked = -not $Packaged
    if ($Packaged) {
        $autostartInput.Enabled = $false
        $autostartInput.Text = "Manage startup in Windows Settings > Apps > Startup"
    }

    $startButton = New-Object System.Windows.Forms.Button
    $startButton.Location = New-Object System.Drawing.Point(312, 192)
    $startButton.Size = New-Object System.Drawing.Size(80, 30)
    $startButton.Text = "Start"
    $startButton.Add_Click({
        if ([string]::IsNullOrWhiteSpace($userInput.Text)) {
            [System.Windows.Forms.MessageBox]::Show(
                "Enter an administrator username.", "GPROXY",
                [System.Windows.Forms.MessageBoxButtons]::OK,
                [System.Windows.Forms.MessageBoxIcon]::Warning) | Out-Null
            return
        }
        if ([string]::IsNullOrWhiteSpace($passwordInput.Text)) {
            [System.Windows.Forms.MessageBox]::Show(
                "Enter an administrator password.", "GPROXY",
                [System.Windows.Forms.MessageBoxButtons]::OK,
                [System.Windows.Forms.MessageBoxIcon]::Warning) | Out-Null
            return
        }
        $form.Tag = @{
            User = $userInput.Text.Trim()
            Password = $passwordInput.Text
            AutoStart = $autostartInput.Checked
        }
        $form.DialogResult = [System.Windows.Forms.DialogResult]::OK
        $form.Close()
    })

    $cancelButton = New-Object System.Windows.Forms.Button
    $cancelButton.Location = New-Object System.Drawing.Point(400, 192)
    $cancelButton.Size = New-Object System.Drawing.Size(80, 30)
    $cancelButton.Text = "Cancel"
    $cancelButton.DialogResult = [System.Windows.Forms.DialogResult]::Cancel

    $form.AcceptButton = $startButton
    $form.CancelButton = $cancelButton
    $form.Controls.AddRange(@(
        $intro, $userLabel, $userInput, $passwordLabel, $passwordInput,
        $autostartInput,
        $startButton, $cancelButton
    ))
    $userInput.Select()

    if ($form.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) {
        return $null
    }
    return $form.Tag
}
if (-not (Test-GproxyHealthy)) {
    $setup = $null
    if ((Test-Path (Join-Path $dataDir ".setup-pending")) -or
        -not (Test-Path (Join-Path $dataDir "gproxy.db"))) {
        $setup = Show-SetupDialog
        if ($null -eq $setup) { exit 0 }
        New-Item -ItemType File -Force -Path (Join-Path $dataDir ".setup-pending") | Out-Null
    }
    $oldUser = $env:GPROXY_ADMIN_USER
    $oldPassword = $env:GPROXY_ADMIN_PASSWORD
    $oldAutostart = $env:GPROXY_AUTOSTART
    try {
        if ($null -ne $setup) {
            $env:GPROXY_ADMIN_USER = $setup.User
            $env:GPROXY_ADMIN_PASSWORD = $setup.Password
            $env:GPROXY_AUTOSTART = $setup.AutoStart.ToString()
        }
        $process = Start-Process -FilePath (Join-Path $installDir "gproxy.exe") `
            -WorkingDirectory $rootDir `
            -WindowStyle Hidden -RedirectStandardOutput (Join-Path $logDir "gproxy.log") `
            -RedirectStandardError (Join-Path $logDir "gproxy-error.log") -PassThru
    } finally {
        $env:GPROXY_ADMIN_USER = $oldUser
        $env:GPROXY_ADMIN_PASSWORD = $oldPassword
        $env:GPROXY_AUTOSTART = $oldAutostart
        if ($null -ne $setup) { $setup.Password = $null }
    }
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

Remove-Item (Join-Path $dataDir ".setup-pending") -ErrorAction SilentlyContinue

if ($OpenConsole) { Start-Process $consoleUrl }
