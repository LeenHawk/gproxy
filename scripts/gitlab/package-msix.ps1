$ErrorActionPreference = 'Stop'
$rows = (Get-Content scripts/release-targets.json -Raw | ConvertFrom-Json).include | Where-Object { $_.os -eq 'windows' }
foreach ($row in $rows) {
    $target = $row.target
    $cliMetadata = Get-Content "dist/release/$($row.artifact).provenance.json" -Raw | ConvertFrom-Json
    if ($cliMetadata.commit -ne $env:CI_COMMIT_SHA) { throw "Wrong CLI source commit for $target" }
    if ($target -eq 'x86_64-pc-windows-msvc') {
        foreach ($name in @('gproxy-unpacked.exe', 'gproxy.exe')) {
            foreach ($argument in @('--version', '--help')) {
                & "./dist/windows/$target/$name" $argument
                if ($LASTEXITCODE -ne 0) { throw "$name failed with $argument on Windows: $LASTEXITCODE" }
            }
        }
    }
    $destination = "target/$target/release"
    New-Item -ItemType Directory -Force $destination | Out-Null
    Copy-Item "dist/windows/$target/*" $destination
    & ./scripts/package-windows-msix.ps1 -Target $target -Artifact $row.artifact -Version $cliMetadata.version -Mode cli -OutputDir dist/msix
    $metadata = Get-Content "dist/release/$($row.application_artifact).provenance.json" -Raw | ConvertFrom-Json
    if ($metadata.commit -ne $env:CI_COMMIT_SHA) { throw "Wrong source commit for $target" }
    & ./scripts/package-application-zip.ps1 -Target $target -Artifact $row.application_artifact -OutputDir dist/msix
    & ./scripts/package-windows-msix.ps1 -Target $target -Artifact $row.application_artifact -Version $metadata.version -OutputDir dist/msix
    $package = "dist/msix/$($row.application_artifact).msix"
    $hash = (Get-FileHash $package -Algorithm SHA256).Hash.ToLower()
    "$hash  $($row.application_artifact).msix" | Set-Content -Encoding ascii "$package.sha256"
}
