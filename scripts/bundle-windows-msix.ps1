param([Parameter(Mandatory)][string]$Version)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Store bundles require a stable version' }
foreach ($arch in @('x86_64', 'aarch64')) {
    if (-not (Test-Path "dist/store/gproxy-tauri-windows-$arch.msix")) { throw "Missing $arch Store package" }
}
$sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$makeappx = Get-ChildItem "$sdkBin/*/x64/makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'Windows SDK makeappx.exe was not found' }
New-Item -ItemType Directory -Force dist/store-upload | Out-Null
& $makeappx.FullName bundle /d dist/store /p dist/store-upload/gproxy-windows.msixbundle /bv "$Version.0" /o
if ($LASTEXITCODE -ne 0) { throw "MSIX bundle packaging failed: $LASTEXITCODE" }
