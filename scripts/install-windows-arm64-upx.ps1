# upx/upx#18927: protect the TLS index and decompress DLLs only on process attach.
# Pin the generated loader artifacts too, not just the ARM64 assembly source.
$ErrorActionPreference = 'Stop'
$revision = '079b95b2d16e8181e6099cb5a674e51d74332793'
$source = Join-Path $env:RUNNER_TEMP 'gproxy-windows-arm64-upx'
$hostArchitecture = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'ARM64' } else { 'x64' }
# The Rust cache includes target/.tools. Keep the pinned native packer there
# instead of recompiling it in each CLI/Application job and every new run.
$packerCache = Join-Path (Split-Path $PSScriptRoot -Parent) "target/.tools/upx-$revision-$hostArchitecture"
$packerExecutable = Join-Path $packerCache 'upx.exe'
function Invoke-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE" }
}
if (-not (Test-Path $packerExecutable)) {
    Invoke-Checked git @('init', $source)
    Invoke-Checked git @('-C', $source, 'remote', 'add', 'origin', 'https://github.com/upx/upx.git')
    Invoke-Checked git @('-C', $source, 'fetch', '--depth', '1', 'origin', $revision)
    Invoke-Checked git @('-C', $source, 'checkout', '--detach', 'FETCH_HEAD')
    Invoke-Checked git @('-C', $source, 'submodule', 'update', '--init', '--recursive', '--depth', '1')
    try {
    Invoke-Checked cmake @('-S', $source, '-B', "$source/build", '-A', $hostArchitecture, '-T', 'host=x64', '-DUPX_CONFIG_DISABLE_WERROR=ON')
    } catch {
        $configureLog = Join-Path $source 'build/CMakeFiles/CMakeConfigureLog.yaml'
        if (Test-Path $configureLog) { Get-Content $configureLog -Tail 160 | Out-Host }
        throw
    }
    Invoke-Checked cmake @('--build', "$source/build", '--config', 'Release', '--target', 'upx', '--parallel', '4')
    New-Item -ItemType Directory -Force -Path $packerCache | Out-Null
    Copy-Item "$source/build/Release/upx.exe" $packerExecutable
}
$packerCache | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
Invoke-Checked $packerExecutable @('--version')
