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
if ($env:TARGET_TRIPLE -eq 'aarch64-pc-windows-msvc') {
    $vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
    $armTools = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath
    if (-not $armTools) {
        $vs = & $vswhere -latest -products '*' -property installationPath
        if (-not $vs) { throw 'Visual Studio Build Tools not found' }
        & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\setup.exe" modify --installPath $vs --add Microsoft.VisualStudio.Component.VC.Tools.ARM64 --quiet --wait --norestart
        if ($LASTEXITCODE -notin @(0, 3010)) { throw "Installing ARM64 tools failed: $LASTEXITCODE" }
    }
}
Run-Checked go @('version')
Run-Checked rustc @('--version')
Run-Checked node @('--version')
