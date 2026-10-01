param([Parameter(Mandatory)][string]$Version)

$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^[1-9]\d*\.\d+\.\d+$') { throw 'Store repackaging requires a stable version' }
New-Item -ItemType Directory -Force dist/portable | Out-Null
gh release download "v$Version" --repo $env:GITHUB_REPOSITORY --pattern "$env:ARTIFACT_PREFIX-*.zip" --pattern SHA256SUMS --dir dist/portable
if ($LASTEXITCODE -ne 0) { throw 'Could not download the stable release packages' }
$checksums = @{}
foreach ($line in Get-Content dist/portable/SHA256SUMS) {
    if ($line -match '^([a-fA-F0-9]{64})\s+\*?(.+)$') { $checksums[$Matches[2]] = $Matches[1] }
}
foreach ($arch in @('x86_64', 'aarch64')) {
    $artifact = "$env:ARTIFACT_PREFIX-$arch"
    $zip = "dist/portable/$artifact.zip"
    if ((Get-FileHash $zip -Algorithm SHA256).Hash -ne $checksums["$artifact.zip"]) {
        throw "Stable release checksum mismatch: $artifact.zip"
    }
    $target = "$arch-pc-windows-msvc"
    $destination = "target/$target/release"
    Expand-Archive -LiteralPath $zip -DestinationPath $destination
    $binary = Join-Path $destination $env:EXECUTABLE
    if ((Get-Item $binary).VersionInfo.ProductVersion -ne $Version) {
        throw "Executable version mismatch: $binary"
    }
    ./scripts/package-windows-msix.ps1 -Target $target -Artifact $artifact -Version $Version -Mode $env:PACKAGE_MODE -IdentityName $env:MS_STORE_IDENTITY_NAME -DisplayName $env:MS_STORE_DISPLAY_NAME
    # Checksums remain internal inputs; MakeAppx's bundle input contains only MSIX files.
    Remove-Item "dist/store/$artifact.msix.sha256"
}
