param(
    [Parameter(Mandatory)][string]$Target,
    [Parameter(Mandatory)][string]$Artifact,
    [Parameter(Mandatory)][string]$Version,
    [Parameter(Mandatory)][string]$IdentityName,
    [Parameter(Mandatory)][string]$DisplayName,
    [Parameter(Mandatory)][string]$Publisher,
    [Parameter(Mandatory)][string]$PublisherDisplayName
)

$ErrorActionPreference = "Stop"
if ($Version -notmatch '^[1-9]\d*\.\d+\.\d+$') { throw "Store packages require a stable release version" }
foreach ($part in $Version.Split('.')) {
    if ([uint32]$part -gt 65535) { throw "MSIX version components must be at most 65535" }
}
if ($IdentityName -notmatch '^[A-Za-z0-9.-]{3,50}$') { throw "Invalid Partner Center identity name" }
if (-not $Publisher.StartsWith('CN=') -or [string]::IsNullOrWhiteSpace($PublisherDisplayName) -or [string]::IsNullOrWhiteSpace($DisplayName)) {
    throw "Partner Center publisher and publisher display name are required"
}
$architecture = switch ($Target) {
    'x86_64-pc-windows-msvc' { 'x64' }
    'aarch64-pc-windows-msvc' { 'arm64' }
    default { throw "Unsupported Windows Store target: $Target" }
}
$sdkBin = Join-Path ${env:ProgramFiles(x86)} "Windows Kits/10/bin"
$makeAppx = Get-ChildItem "$sdkBin/*/x64/makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending |
    Select-Object -First 1
if (-not $makeAppx) { throw "Windows SDK MakeAppx was not found" }

$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
$package = Join-Path $work 'package'
try {
    Expand-Archive -LiteralPath "dist/release/$Artifact.zip" -DestinationPath $package
    Copy-Item scripts/installers/windows/GPROXY.ps1 $package
    $launcher = Join-Path $package 'gproxy-launcher.exe'
    & rustc --edition=2024 --target $Target -C opt-level=z -C panic=abort `
        --crate-name gproxy_store_launcher scripts/installers/windows/launcher.rs -o $launcher
    if ($LASTEXITCODE -ne 0) { throw "Store launcher build failed: $LASTEXITCODE" }

    Add-Type -AssemblyName System.Drawing
    $assets = New-Item -ItemType Directory -Path (Join-Path $package 'Assets')
    $icon = [System.Drawing.Image]::FromFile((Resolve-Path 'console/public/apple-touch-icon.png'))
    try {
        foreach ($item in @(@('StoreLogo', 50), @('Square44x44Logo', 44), @('Square150x150Logo', 150))) {
            $bitmap = [System.Drawing.Bitmap]::new([int]$item[1], [int]$item[1])
            $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
            try {
                $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
                $graphics.DrawImage($icon, 0, 0, $bitmap.Width, $bitmap.Height)
                $bitmap.Save((Join-Path $assets.FullName "$($item[0]).png"), [System.Drawing.Imaging.ImageFormat]::Png)
            } finally { $graphics.Dispose(); $bitmap.Dispose() }
        }
    } finally { $icon.Dispose() }

    $manifest = Get-Content scripts/installers/windows/AppxManifest.xml.in -Raw
    $values = @{
        IDENTITY_NAME = $IdentityName
        DISPLAY_NAME = $DisplayName
        PUBLISHER = $Publisher
        PUBLISHER_DISPLAY_NAME = $PublisherDisplayName
        VERSION = "$Version.0"
        ARCHITECTURE = $architecture
    }
    foreach ($key in $values.Keys) {
        $manifest = $manifest.Replace("__$($key)__", [System.Security.SecurityElement]::Escape($values[$key]))
    }
    [System.IO.File]::WriteAllText((Join-Path $package 'AppxManifest.xml'), $manifest, [System.Text.UTF8Encoding]::new($false))
    New-Item -ItemType Directory -Force -Path dist/store | Out-Null
    $output = Join-Path (Resolve-Path dist/store) "$Artifact.msix"
    & $makeAppx.FullName pack /d $package /p $output /o /h SHA256
    if ($LASTEXITCODE -ne 0) { throw "MSIX packaging/validation failed: $LASTEXITCODE" }
    Write-Output "Created unsigned Store submission: $output (Microsoft signs after certification)"
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
