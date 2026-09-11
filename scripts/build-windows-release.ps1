param([Parameter(Mandatory)][string]$Target)

$ErrorActionPreference = "Stop"
$version = $env:GPROXY_BUILD_VERSION
if (-not $version) { throw "GPROXY_BUILD_VERSION is required" }
if ($version -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
    throw "Invalid release version: $version"
}
$numericVersion = (@(1, 2, 3) | ForEach-Object { [uint16]$Matches[$_] }) -join ','
$sdkBin = Join-Path ${env:ProgramFiles(x86)} "Windows Kits/10/bin"
$compiler = Get-ChildItem "$sdkBin/*/x64/rc.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
    Select-Object -First 1
if (-not $compiler) { throw "Windows SDK resource compiler was not found" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
try {
    New-Item -ItemType Directory -Path $work | Out-Null
    $resource = Join-Path $work "gproxy.res"
    $source = Join-Path $work "gproxy.rc"
    $template = Get-Content scripts/installers/windows/gproxy.rc.in -Raw
    $template.Replace("__VERSION__", $version).Replace("__NUMERIC_VERSION__", "$numericVersion,0") |
        Set-Content -Encoding ascii $source
    & $compiler.FullName /nologo /fo $resource $source
    if ($LASTEXITCODE -ne 0) { throw "Windows resource compilation failed: $LASTEXITCODE" }
    & cargo rustc --locked --release -p gproxy-host-axum --bin gproxy --target $Target -- -C "link-arg=$resource"
    if ($LASTEXITCODE -ne 0) { throw "Windows release build failed: $LASTEXITCODE" }
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
