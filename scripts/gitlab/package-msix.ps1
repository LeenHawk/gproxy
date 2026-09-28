$ErrorActionPreference = 'Stop'
$rows = (Get-Content scripts/release-targets.json -Raw | ConvertFrom-Json).include | Where-Object { $_.os -eq 'windows' }
foreach ($row in $rows) {
    $target = $row.target
    $destination = "target/$target/release"
    New-Item -ItemType Directory -Force $destination | Out-Null
    Copy-Item "dist/windows/$target/*" $destination
    $metadata = Get-Content "dist/release/$($row.application_artifact).provenance.json" -Raw | ConvertFrom-Json
    if ($metadata.commit -ne $env:CI_COMMIT_SHA) { throw "Wrong source commit for $target" }
    & ./scripts/package-windows-msix.ps1 -Target $target -Artifact $row.application_artifact -Version $metadata.version -OutputDir dist/msix
    $package = "dist/msix/$($row.application_artifact).msix"
    $hash = (Get-FileHash $package -Algorithm SHA256).Hash.ToLower()
    "$hash  $($row.application_artifact).msix" | Set-Content -Encoding ascii "$package.sha256"
}
