# upx/upx#18927: protect the TLS index and decompress DLLs only on process attach.
# Pin the generated loader artifacts too, not just the ARM64 assembly source.
$ErrorActionPreference = 'Stop'
$revision = '079b95b2d16e8181e6099cb5a674e51d74332793'
$source = Join-Path $env:RUNNER_TEMP 'gproxy-windows-arm64-upx'
function Invoke-Checked {
    param([string]$Command, [string[]]$Arguments)
    & $Command @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Command failed with exit code $LASTEXITCODE" }
}
Invoke-Checked git @('init', $source)
Invoke-Checked git @('-C', $source, 'remote', 'add', 'origin', 'https://github.com/upx/upx.git')
Invoke-Checked git @('-C', $source, 'fetch', '--depth', '1', 'origin', $revision)
Invoke-Checked git @('-C', $source, 'checkout', '--detach', 'FETCH_HEAD')
Invoke-Checked git @('-C', $source, 'submodule', 'update', '--init', '--recursive', '--depth', '1')
$hostArchitecture = if ([System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -eq 'Arm64') { 'ARM64' } else { 'x64' }
Invoke-Checked cmake @('-S', $source, '-B', "$source/build", '-A', $hostArchitecture, '-DUPX_CONFIG_DISABLE_WERROR=ON')
Invoke-Checked cmake @('--build', "$source/build", '--config', 'Release', '--target', 'upx', '--parallel', '4')
"$source/build/Release" | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
Invoke-Checked "$source/build/Release/upx.exe" @('--version')
