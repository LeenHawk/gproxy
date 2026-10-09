param([Parameter(Mandatory)][ValidateSet('cli', 'application')][string]$Mode)
$ErrorActionPreference = 'Stop'
function Run-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE" }
}
$target = $env:TARGET_TRIPLE
$row = (Get-Content scripts/release-targets.json -Raw | ConvertFrom-Json).include | Where-Object { $_.target -eq $target }
if (-not $row) { throw "Unknown target: $target" }
$env:TARGET_OS = 'windows'
$env:ARTIFACT_NAME = $row.artifact
$env:BUILDER = 'cargo'
$env:UPX_ENABLED = 'true'
$env:RUSTFLAGS = '-C target-feature=+crt-static'
$env:GPROXY_BUILD_VERSION = (& bash --login scripts/release-metadata.sh version).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Could not read version' }
$env:GPROXY_BUILD_HASH = $env:CI_COMMIT_SHA
$env:GPROXY_BUILD_CHANNEL = 'dev'
$env:GPROXY_BUILD_UPDATE_SOURCE = 'github'
$env:GPROXY_UPDATE_PUBKEY = (Get-Content .gitlab/update-public-key -Raw).Trim()
$env:GPROXY_INSTALLATION_KIND = 'standalone'
$env:GITHUB_SHA = $env:CI_COMMIT_SHA
$env:GITHUB_REF_NAME = 'nightly'
if ($env:CI_COMMIT_TAG) {
    Run-Checked bash @('--login', 'scripts/release-metadata.sh', 'verify-tag', $env:CI_COMMIT_TAG)
    $env:GITHUB_REF_NAME = $env:CI_COMMIT_TAG
    $env:GPROXY_BUILD_CHANNEL = if ($env:GPROXY_BUILD_VERSION.Contains('-')) { 'beta' } else { 'release' }
}
$env:GPROXY_VERSION = $env:GPROXY_BUILD_VERSION
$env:RUNNER_TEMP = Join-Path $env:CI_PROJECT_DIR '.ci-tmp'
New-Item -ItemType Directory -Force $env:RUNNER_TEMP | Out-Null
$env:GITHUB_PATH = Join-Path $env:RUNNER_TEMP 'paths'
if (-not (Test-Path $env:GITHUB_PATH)) { New-Item -ItemType File $env:GITHUB_PATH | Out-Null }
if ($target -eq 'aarch64-pc-windows-msvc') {
    $env:CMAKE_TOOLCHAIN_FILE_aarch64_pc_windows_msvc = (Resolve-Path scripts/cmake/windows-arm64.cmake).Path
    if (-not (Test-Path "$env:RUNNER_TEMP/gproxy-windows-arm64-upx/build/Release/upx.exe")) {
        & scripts/install-windows-arm64-upx.ps1
    }
} else {
    Run-Checked bash @('--login', 'scripts/install-upx.sh')
}
foreach ($directory in Get-Content $env:GITHUB_PATH) { $env:PATH = "$directory;$env:PATH" }

if ($Mode -eq 'cli') {
    Write-Host "Building CLI / server: $env:ARTIFACT_NAME"
    & scripts/build-windows-release.ps1 -Target $target
    if ($target -eq 'x86_64-pc-windows-msvc') {
        & scripts/pack-windows-cli.ps1 -Target $target
    } else {
        Run-Checked upx @('--best', '--lzma', "target/$target/release/gproxy.exe")
        Run-Checked upx @('--test', "target/$target/release/gproxy.exe")
    }
    & scripts/package-native-release.ps1
} else {
    $env:ARTIFACT_NAME = $row.application_artifact
    $env:BUILDER = 'tauri'
    Write-Host "Building Application: $env:ARTIFACT_NAME"
    Run-Checked pnpm @('--dir', 'crates/gproxy-host-tauri', 'install', '--frozen-lockfile')
    Run-Checked bash @('--login', 'scripts/package-tauri-release.sh')
}
Run-Checked bash @('--login', 'scripts/build-provenance.sh')
