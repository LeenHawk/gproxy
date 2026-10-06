param(
    [Parameter(Mandatory)][string]$Target,
    [Parameter(Mandatory)][string]$Artifact,
    [Parameter(Mandatory)][string]$Version,
    [string]$OutputDir = 'dist/store',
    [ValidateSet('cli', 'application')][string]$Mode = 'application',
    [string]$IdentityName = $(if ($Mode -eq 'cli') { $env:MS_STORE_CLI_IDENTITY_NAME } else { $env:MS_STORE_IDENTITY_NAME }),
    [string]$DisplayName = $(if ($Mode -eq 'cli') { $env:MS_STORE_CLI_DISPLAY_NAME } else { $env:MS_STORE_DISPLAY_NAME }),
    [string]$Publisher = $env:MS_STORE_IDENTITY_PUBLISHER,
    [string]$PublisherDisplayName = $env:MS_STORE_PUBLISHER_DISPLAY_NAME
)
$ErrorActionPreference = 'Stop'
foreach ($value in @($IdentityName, $DisplayName, $Publisher, $PublisherDisplayName)) {
    if ([string]::IsNullOrWhiteSpace($value)) { throw "The $Mode Store identity, display name, publisher and publisher display name are required" }
}
if ($Version -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-[0-9A-Za-z.-]+)?$') { throw "Invalid version: $Version" }
$packageVersion = (@(1, 2, 3) | ForEach-Object { [uint16]$Matches[$_] }) -join '.'
$packageVersion += '.0'
$arch = switch ($Target) {
    'x86_64-pc-windows-msvc' { 'x64' }
    'aarch64-pc-windows-msvc' { 'arm64' }
    default { throw "Unsupported MSIX target: $Target" }
}
$sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$makeappx = Get-ChildItem "$sdkBin/*/x64/makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'Windows SDK makeappx.exe was not found' }
$executable = if ($Mode -eq 'cli') { 'gproxy.exe' } else { 'gproxy-desktop.exe' }
$binary = "target/$Target/release/$executable"
if (-not (Test-Path $binary)) { throw "Missing executable: $binary" }
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
try {
    New-Item -ItemType Directory -Force -Path "$work/Assets", $OutputDir | Out-Null
    Copy-Item $binary "$work/$executable"
    # Tauri can resolve WebView2 beside its executable; include it if emitted.
    $loader = "target/$Target/release/WebView2Loader.dll"
    if ($Mode -eq 'application' -and (Test-Path $loader)) { Copy-Item $loader $work }
    foreach ($logo in @('StoreLogo.png', 'Square44x44Logo.png', 'Square150x150Logo.png')) {
        Copy-Item "crates/gproxy-host-tauri/icons/$logo" "$work/Assets/$logo"
    }
    $identityXml = [System.Security.SecurityElement]::Escape($IdentityName)
    $displayXml = [System.Security.SecurityElement]::Escape($DisplayName)
    $publisherXml = [System.Security.SecurityElement]::Escape($Publisher)
    $publisherDisplayXml = [System.Security.SecurityElement]::Escape($PublisherDisplayName)
    $applicationAttributes = if ($Mode -eq 'cli') { 'desktop4:SupportsMultipleInstances="true"' } else { '' }
    $extensions = if ($Mode -eq 'cli') {
        '<uap5:Extension Category="windows.appExecutionAlias" Executable="gproxy.exe" EntryPoint="Windows.FullTrustApplication"><uap5:AppExecutionAlias desktop4:Subsystem="console"><uap5:ExecutionAlias Alias="gproxy.exe" /></uap5:AppExecutionAlias></uap5:Extension>'
    } else {
        "<desktop:Extension Category=`"windows.startupTask`" uap10:Parameters=`"--autostart`" Executable=`"gproxy-desktop.exe`" EntryPoint=`"Windows.FullTrustApplication`"><desktop:StartupTask TaskId=`"GproxyStartup`" Enabled=`"false`" DisplayName=`"$displayXml`" /></desktop:Extension>"
    }
    @"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
 xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
 xmlns:desktop="http://schemas.microsoft.com/appx/manifest/desktop/windows10"
 xmlns:desktop4="http://schemas.microsoft.com/appx/manifest/desktop/windows10/4"
 xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
 xmlns:uap5="http://schemas.microsoft.com/appx/manifest/uap/windows10/5"
 xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
 IgnorableNamespaces="uap uap10 uap5 desktop desktop4 rescap">
 <Identity Name="$identityXml" Publisher="$publisherXml" Version="$packageVersion" ProcessorArchitecture="$arch" />
 <Properties><DisplayName>$displayXml</DisplayName><PublisherDisplayName>$publisherDisplayXml</PublisherDisplayName><Logo>Assets\StoreLogo.png</Logo></Properties>
 <Dependencies><TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.17763.0" MaxVersionTested="10.0.26100.0" /></Dependencies>
 <Resources><Resource Language="en-us" /><Resource Language="zh-cn" /><Resource Language="zh-tw" /></Resources>
 <Applications><Application Id="GPROXY" Executable="$executable" EntryPoint="Windows.FullTrustApplication" $applicationAttributes>
  <uap:VisualElements DisplayName="$displayXml" Description="$displayXml" BackgroundColor="transparent" Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png" />
  <Extensions>$extensions</Extensions>
 </Application></Applications>
 <Capabilities><rescap:Capability Name="runFullTrust" /></Capabilities>
</Package>
"@ | Set-Content -Encoding utf8 "$work/AppxManifest.xml"
    $package = Join-Path $OutputDir "$Artifact.msix"
    & python "$PSScriptRoot/reproducible-env.py" --normalize-tree $work
    if ($LASTEXITCODE -ne 0) { throw "MSIX timestamp normalization failed: $LASTEXITCODE" }
    & $makeappx.FullName pack /d $work /p $package /o
    if ($LASTEXITCODE -ne 0) { throw "MSIX packaging failed: $LASTEXITCODE" }
    & python "$PSScriptRoot/reproducible-zip-timestamps.py" $package
    if ($LASTEXITCODE -ne 0) { throw "MSIX ZIP timestamp normalization failed: $LASTEXITCODE" }
    $hash = (Get-FileHash $package -Algorithm SHA256).Hash.ToLower()
    "$hash  $Artifact.msix" | Set-Content -Encoding ascii "$package.sha256"
} finally { Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue }
