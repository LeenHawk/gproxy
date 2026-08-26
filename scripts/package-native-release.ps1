$ErrorActionPreference = "Stop"

$target = $env:TARGET_TRIPLE
$artifact = $env:ARTIFACT_NAME
$outputDir = if ($env:OUTPUT_DIR) { $env:OUTPUT_DIR } else { "dist/release" }
if (-not $target -or -not $artifact) {
    throw "TARGET_TRIPLE and ARTIFACT_NAME are required"
}

$binary = "target/$target/release/gproxy.exe"
if (-not (Test-Path $binary)) { throw "missing release binary: $binary" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
$package = Join-Path $work $artifact
try {
    New-Item -ItemType Directory -Force -Path $package, $outputDir | Out-Null
    Copy-Item $binary "$package/gproxy.exe"
    Copy-Item README.md, LICENSE $package
    $archive = Join-Path $outputDir "$artifact.zip"
    Remove-Item $archive, "$archive.sha256" -Force -ErrorAction SilentlyContinue
    Compress-Archive -Path "$package/*" -DestinationPath $archive
    $hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
    "$hash  $artifact.zip" | Out-File -Encoding ascii "$archive.sha256"

    Copy-Item scripts/installers/windows/GPROXY.vbs, scripts/installers/windows/GPROXY.ps1 $package
    $version = (& bash scripts/release-metadata.sh version).Trim()
    $msiVersion = $version.Split("-")[0]
    $source = $package
    $wxs = (Get-Content scripts/installers/windows/Package.wxs.in -Raw).
        Replace("__VERSION__", $msiVersion).Replace("__SOURCE__", $source)
    $wxsPath = Join-Path $work "Package.wxs"
    [System.IO.File]::WriteAllText($wxsPath, $wxs, ([System.Text.UTF8Encoding]::new($false)))
    $architecture = if ($target.StartsWith("aarch64")) { "arm64" } else { "x64" }
    $msi = Join-Path $outputDir "$artifact.msi"
    & wix build -arch $architecture -o $msi $wxsPath
    if ($LASTEXITCODE -ne 0) { throw "WiX packaging failed with exit code $LASTEXITCODE" }
    $msiHash = (Get-FileHash $msi -Algorithm SHA256).Hash.ToLower()
    "$msiHash  $artifact.msi" | Out-File -Encoding ascii "$msi.sha256"
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
