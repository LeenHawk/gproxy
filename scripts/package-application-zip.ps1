param(
    [Parameter(Mandatory)][string]$Target,
    [Parameter(Mandatory)][string]$Artifact,
    [string]$OutputDir = 'dist/release'
)
$ErrorActionPreference = 'Stop'
$release = "target/$Target/release"
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
try {
    New-Item -ItemType Directory -Force $work, $OutputDir | Out-Null
    Copy-Item "$release/gproxy-desktop.exe" $work
    Get-ChildItem "$release/*.dll" | Copy-Item -Destination $work
    Copy-Item README.md, LICENSE $work
    'Run gproxy-desktop.exe. Microsoft Edge WebView2 Runtime is required (https://developer.microsoft.com/microsoft-edge/webview2/).' | Set-Content -Encoding utf8 "$work/RUN.txt"
    $archive = Join-Path $OutputDir "$Artifact.zip"
    & python "$PSScriptRoot/reproducible-archive.py" --root $work --output $archive
    if ($LASTEXITCODE -ne 0) { throw "ZIP packaging failed: $LASTEXITCODE" }
    $hash = (Get-FileHash $archive -Algorithm SHA256).Hash.ToLower()
    "$hash  $Artifact.zip" | Set-Content -Encoding ascii "$archive.sha256"
} finally { Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue }
