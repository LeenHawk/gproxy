# Keep compression enabled while testing UPX's corrected ARM64 entry stub.
# upx/upx#18909: preserve x0-x3 through decompression and TLS callbacks.
$ErrorActionPreference = 'Stop'
$revision = 'b888ad87f5d7d8d890777b03d71f09d01a8eb902'
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
