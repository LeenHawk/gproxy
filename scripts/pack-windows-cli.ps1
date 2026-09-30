param([Parameter(Mandatory)][string]$Target)
$ErrorActionPreference = 'Stop'
$binary = (Resolve-Path "target/$Target/release/gproxy.exe").Path

function Test-Executable {
    foreach ($argument in @('--version', '--help')) {
        & $binary $argument | Out-Host
        if ($LASTEXITCODE -ne 0) {
            Write-Host "Executable failed with $argument, exit code $LASTEXITCODE"
            return $false
        }
    }
    return $true
}

Write-Host 'Smoke checking the uncompressed Windows executable'
if (-not (Test-Executable)) { throw 'Uncompressed Windows executable failed its smoke check' }
# ARM64 --best spent almost 14 minutes compressing a 49 MiB CLI on the native runner.
$packArguments = if ($Target -eq 'aarch64-pc-windows-msvc') { @('--fast', '--nrv2e') } else { @('--best', '--lzma') }
Write-Host "Compressing with UPX $packArguments"
& upx @packArguments $binary
if ($LASTEXITCODE -ne 0) { throw 'UPX compression failed' }
Write-Host 'Checking UPX integrity'
& upx --test $binary
if ($LASTEXITCODE -ne 0) { throw 'UPX integrity check failed' }
Write-Host 'Smoke checking the UPX-packed Windows executable'
if (-not (Test-Executable)) { throw 'UPX-packed Windows executable failed its smoke check' }
