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

if (-not (Test-Executable)) { throw 'Uncompressed Windows executable failed its smoke check' }
& upx --best --lzma $binary
if ($LASTEXITCODE -ne 0) { throw 'UPX compression failed' }
& upx --test $binary
if ($LASTEXITCODE -ne 0) { throw 'UPX integrity check failed' }
if (-not (Test-Executable)) { throw 'UPX-packed Windows executable failed its smoke check' }
