$ErrorActionPreference = "Stop"

$target = $env:TARGET_TRIPLE
$artifact = $env:ARTIFACT_NAME
if (-not $target -or -not $artifact) { throw "TARGET_TRIPLE and ARTIFACT_NAME are required" }

$binary = "target/$target/release/gproxy.exe"
$versionLine = Select-String -Path Cargo.toml -Pattern '^version = "([^"]+)"' | Select-Object -First 1
$version = $versionLine.Matches[0].Groups[1].Value
$source = Join-Path $PWD "dist-windows"
New-Item -ItemType Directory -Force -Path $source | Out-Null
Copy-Item $binary "$source/gproxy.exe"
Copy-Item README.md "$source/README.md"
Copy-Item scripts/installers/windows/GPROXY.vbs "$source/GPROXY.vbs"

Compress-Archive -Path "$source/gproxy.exe","$source/README.md" `
  -DestinationPath "$artifact.zip" -Force

$wxs = Get-Content scripts/installers/windows/Package.wxs.in -Raw
$wxs = $wxs.Replace("__VERSION__", $version).Replace("__SOURCE__", $source)
$wxsPath = Join-Path $source "Package.wxs"
Set-Content -Path $wxsPath -Value $wxs -Encoding utf8
$arch = if ($target.StartsWith("aarch64")) { "arm64" } else { "x64" }
& wix build -arch $arch -o "$artifact.msi" $wxsPath
if ($LASTEXITCODE -ne 0) { throw "WiX packaging failed with exit code $LASTEXITCODE" }

foreach ($file in @("$artifact.zip", "$artifact.msi")) {
  $hash = (Get-FileHash $file -Algorithm SHA256).Hash.ToLower()
  "$hash  $file" | Out-File -Encoding ascii "$file.sha256"
}
