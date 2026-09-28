$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
function Run-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE" }
}
$toolsDir = Join-Path $env:CI_PROJECT_DIR '.ci-tools'
New-Item -ItemType Directory -Force $toolsDir | Out-Null
Run-Checked git @('config', '--global', 'core.longpaths', 'true')
$gitRoot = Split-Path (Split-Path (Get-Command git).Source -Parent) -Parent
$bashDirectories = @("$gitRoot\bin", "${env:ProgramW6432}\Git\bin", "${env:ProgramFiles(x86)}\Git\bin")
$bashDirectory = $bashDirectories | Where-Object { Test-Path "$_\bash.exe" } | Select-Object -First 1
if (-not $bashDirectory) {
    Run-Checked choco @('install', 'git.install', '-y', '--no-progress')
    $bashDirectory = $bashDirectories | Where-Object { Test-Path "$_\bash.exe" } | Select-Object -First 1
}
if (-not $bashDirectory) { throw 'Git for Windows Bash was not installed' }
$env:PATH = "$bashDirectory;$env:PATH"
$env:CHERE_INVOKING = '1'
Run-Checked bash @('--login', '-c', 'command -v awk && test -f scripts/release-metadata.sh')
if (-not (Get-Command pwsh -ErrorAction SilentlyContinue)) {
    Run-Checked choco @('install', 'powershell-core', '-y', '--no-progress')
}
$env:PATH = "C:\Program Files\PowerShell\7;C:\Program Files\LLVM\bin;$env:PATH"
if (-not (Get-Command clang -ErrorAction SilentlyContinue)) {
    Run-Checked choco @('install', 'llvm', '-y', '--no-progress')
}
$llvmDirectories = @("$env:ProgramW6432\LLVM\bin", "${env:ProgramFiles(x86)}\LLVM\bin")
$env:LIBCLANG_PATH = $llvmDirectories | Where-Object { Test-Path "$_\libclang.dll" } | Select-Object -First 1
if (-not $env:LIBCLANG_PATH) { throw 'LLVM installed without libclang.dll' }
$env:PATH = "$env:LIBCLANG_PATH;$env:PATH"
$env:PATH = "${env:ProgramW6432}\PowerShell\7;$env:PATH"
Write-Host "Installing Node $env:NODE_VERSION and Go $env:GO_VERSION"
$nodeArchive = "node-v$env:NODE_VERSION-win-x64.zip"
Invoke-WebRequest "https://nodejs.org/dist/v$env:NODE_VERSION/$nodeArchive" -OutFile "$toolsDir/$nodeArchive" -UseBasicParsing
Invoke-WebRequest "https://nodejs.org/dist/v$env:NODE_VERSION/SHASUMS256.txt" -OutFile "$toolsDir/node-shasums" -UseBasicParsing
$expected = ((Get-Content "$toolsDir/node-shasums" | Where-Object { $_ -match "\s+$([regex]::Escape($nodeArchive))$" }) -split '\s+')[0]
if ((Get-FileHash "$toolsDir/$nodeArchive" -Algorithm SHA256).Hash.ToLower() -ne $expected) { throw 'Node checksum mismatch' }
Expand-Archive "$toolsDir/$nodeArchive" $toolsDir -Force
Invoke-WebRequest "https://go.dev/dl/go$env:GO_VERSION.windows-amd64.zip" -OutFile "$toolsDir/go.zip" -UseBasicParsing
Expand-Archive "$toolsDir/go.zip" $toolsDir -Force
$env:GOROOT = "$toolsDir\go"
$env:PATH = "$toolsDir\node-v$env:NODE_VERSION-win-x64;$toolsDir\go\bin;$env:CARGO_HOME\bin;$env:USERPROFILE\.cargo\bin;$env:PATH"
Run-Checked npm @('install', '-g', 'pnpm@9.15.9')
$npmPrefix = (& npm prefix -g).Trim()
$env:PATH = "$npmPrefix;$env:PATH"
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile "$toolsDir/rustup-init.exe" -UseBasicParsing
    Run-Checked "$toolsDir/rustup-init.exe" @('-y', '--profile', 'minimal', '--default-toolchain', $env:RUST_VERSION)
}
Run-Checked rustup @('toolchain', 'install', $env:RUST_VERSION, '--profile', 'minimal')
Run-Checked rustup @('default', $env:RUST_VERSION)
$env:RUSTUP_TOOLCHAIN = $env:RUST_VERSION
Run-Checked rustup @('target', 'add', $env:TARGET_TRIPLE)
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$vs = & $vswhere -latest -products '*' -property installationPath
if (-not $vs) { throw 'Visual Studio Build Tools not found' }
$targetArch = 'x64'
if ($env:TARGET_TRIPLE -eq 'aarch64-pc-windows-msvc') {
    $targetArch = 'arm64'
    $armTools = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath
    if (-not $armTools) {
        $installer = Start-Process -FilePath "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\setup.exe" -ArgumentList "modify --installPath `"$vs`" --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --quiet --norestart" -Wait -PassThru
        if ($installer.ExitCode -notin @(0, 3010)) { throw "Installing ARM64 tools failed: $($installer.ExitCode)" }
    }
}
Import-Module "$vs\Common7\Tools\Microsoft.VisualStudio.DevShell.dll"
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments "-arch=$targetArch -host_arch=x64"
Get-Command cl.exe -ErrorAction Stop | Select-Object -ExpandProperty Source
Run-Checked go @('version')
Run-Checked rustc @('--version')
Run-Checked node @('--version')
Run-Checked bash @('--login', '-c', 'for tool in awk cmake cargo pnpm; do command -v "$tool" || exit 1; done')
