param(
    [switch]$OpenConsole
)

$ErrorActionPreference = "Stop"

$installDir = Join-Path $env:LOCALAPPDATA "Programs\GPROXY"
$rootDir = Join-Path $env:LOCALAPPDATA "GPROXY"
$dataDir = Join-Path $rootDir "data"
$logDir = Join-Path $rootDir "logs"
$adminMarker = Join-Path $dataDir ".desktop-admin-user"
$autostartMarker = Join-Path $dataDir ".autostart-initialized"
$startupDir = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs\Startup"
$startupScript = Join-Path $startupDir "GPROXY.vbs"
$consoleUrl = "http://127.0.0.1:8787/console"
$healthUrl = $consoleUrl

New-Item -ItemType Directory -Force -Path $dataDir, $logDir | Out-Null

function Test-GproxyHealthy {
    try {
        $request = [System.Net.WebRequest]::Create($healthUrl)
        $request.Timeout = 700
        $response = $request.GetResponse()
        $response.Close()
        return $true
    } catch {
        return $false
    }
}

function Open-GproxyConsole {
    Start-Process $consoleUrl
}

function Show-SetupDialog {
    Add-Type -AssemblyName System.Windows.Forms
    Add-Type -AssemblyName System.Drawing

    $form = New-Object System.Windows.Forms.Form
    $form.Text = "Set up GPROXY"
    $form.ClientSize = New-Object System.Drawing.Size(440, 250)
    $form.StartPosition = "CenterScreen"
    $form.FormBorderStyle = "FixedDialog"
    $form.MaximizeBox = $false
    $form.MinimizeBox = $false

    $intro = New-Object System.Windows.Forms.Label
    $intro.Location = New-Object System.Drawing.Point(20, 18)
    $intro.Size = New-Object System.Drawing.Size(400, 42)
    $intro.Text = "Choose the administrator credentials used to sign in to Console. GPROXY stores the password hash; this launcher does not save the plaintext."

    $userLabel = New-Object System.Windows.Forms.Label
    $userLabel.Location = New-Object System.Drawing.Point(20, 72)
    $userLabel.Size = New-Object System.Drawing.Size(130, 20)
    $userLabel.Text = "Admin username"

    $userInput = New-Object System.Windows.Forms.TextBox
    $userInput.Location = New-Object System.Drawing.Point(160, 69)
    $userInput.Size = New-Object System.Drawing.Size(260, 24)
    $userInput.Text = "admin"

    $passwordLabel = New-Object System.Windows.Forms.Label
    $passwordLabel.Location = New-Object System.Drawing.Point(20, 108)
    $passwordLabel.Size = New-Object System.Drawing.Size(130, 20)
    $passwordLabel.Text = "Admin password"

    $passwordInput = New-Object System.Windows.Forms.TextBox
    $passwordInput.Location = New-Object System.Drawing.Point(160, 105)
    $passwordInput.Size = New-Object System.Drawing.Size(260, 24)
    $passwordInput.UseSystemPasswordChar = $true

    $autostartInput = New-Object System.Windows.Forms.CheckBox
    $autostartInput.Location = New-Object System.Drawing.Point(20, 147)
    $autostartInput.Size = New-Object System.Drawing.Size(400, 24)
    $autostartInput.Text = "Start GPROXY automatically when I sign in"
    $autostartInput.Checked = $true

    $startButton = New-Object System.Windows.Forms.Button
    $startButton.Location = New-Object System.Drawing.Point(252, 196)
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
    $cancelButton.Location = New-Object System.Drawing.Point(340, 196)
    $cancelButton.Size = New-Object System.Drawing.Size(80, 30)
    $cancelButton.Text = "Cancel"
    $cancelButton.DialogResult = [System.Windows.Forms.DialogResult]::Cancel

    $form.AcceptButton = $startButton
    $form.CancelButton = $cancelButton
    $form.Controls.AddRange(@(
        $intro, $userLabel, $userInput, $passwordLabel, $passwordInput,
        $autostartInput, $startButton, $cancelButton
    ))
    $userInput.Select()

    if ($form.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK) {
        return $null
    }
    return $form.Tag
}

function Start-GproxyProcess([string]$adminUser, [string]$adminPassword) {
    $exe = Join-Path $installDir "gproxy.exe"
    if (-not (Test-Path $exe)) {
        throw "GPROXY executable not found at $exe"
    }

    $oldUser = $env:GPROXY_ADMIN_USER
    $oldPassword = $env:GPROXY_ADMIN_PASSWORD
    try {
        if ($adminUser) { $env:GPROXY_ADMIN_USER = $adminUser }
        if ($adminPassword) { $env:GPROXY_ADMIN_PASSWORD = $adminPassword }
        return Start-Process -FilePath $exe `
            -ArgumentList @("--data-dir", ('"{0}"' -f $dataDir)) `
            -WorkingDirectory $rootDir `
            -WindowStyle Hidden `
            -RedirectStandardOutput (Join-Path $logDir "gproxy.log") `
            -RedirectStandardError (Join-Path $logDir "gproxy-error.log") `
            -PassThru
    } finally {
        if ($null -eq $oldUser) {
            Remove-Item Env:GPROXY_ADMIN_USER -ErrorAction SilentlyContinue
        } else {
            $env:GPROXY_ADMIN_USER = $oldUser
        }
        if ($null -eq $oldPassword) {
            Remove-Item Env:GPROXY_ADMIN_PASSWORD -ErrorAction SilentlyContinue
        } else {
            $env:GPROXY_ADMIN_PASSWORD = $oldPassword
        }
    }
}

if (Test-GproxyHealthy) {
    if (-not (Test-Path $adminMarker)) {
        Add-Type -AssemblyName System.Windows.Forms
        [System.Windows.Forms.MessageBox]::Show(
            "GPROXY is already running, so first-run setup cannot safely update its administrator. Stop the existing GPROXY process and open the launcher again.",
            "GPROXY setup", [System.Windows.Forms.MessageBoxButtons]::OK,
            [System.Windows.Forms.MessageBoxIcon]::Warning) | Out-Null
        exit 1
    }
    if ($OpenConsole) { Open-GproxyConsole }
    exit 0
}

$setup = $null
if (-not (Test-Path $adminMarker)) {
    $setup = Show-SetupDialog
    if ($null -eq $setup) { exit 0 }
}

$process = if ($null -eq $setup) {
    Start-GproxyProcess "" ""
} else {
    Start-GproxyProcess $setup.User $setup.Password
}

$healthy = $false
for ($attempt = 0; $attempt -lt 75; $attempt++) {
    Start-Sleep -Milliseconds 200
    if (Test-GproxyHealthy) {
        $healthy = $true
        break
    }
    if ($process.HasExited) { break }
}

if (-not $healthy) {
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force }
    Add-Type -AssemblyName System.Windows.Forms
    [System.Windows.Forms.MessageBox]::Show(
        "GPROXY did not start. Check the logs in $logDir.", "GPROXY",
        [System.Windows.Forms.MessageBoxButtons]::OK,
        [System.Windows.Forms.MessageBoxIcon]::Error) | Out-Null
    exit 1
}

if ($null -ne $setup) {
    [System.IO.File]::WriteAllText(
        $adminMarker, $setup.User, ([System.Text.UTF8Encoding]::new($false)))
    [System.IO.File]::WriteAllText(
        $autostartMarker, "1`n", ([System.Text.UTF8Encoding]::new($false)))
    if ($setup.AutoStart) {
        New-Item -ItemType Directory -Force -Path $startupDir | Out-Null
        Copy-Item (Join-Path $installDir "GPROXY.vbs") $startupScript -Force
    } else {
        Remove-Item $startupScript -Force -ErrorAction SilentlyContinue
    }
}

if ($OpenConsole) { Open-GproxyConsole }
