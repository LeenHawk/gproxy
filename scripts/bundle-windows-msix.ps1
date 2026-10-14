param(
    [Parameter(Mandatory)][string]$Version,
    [string]$ArtifactPrefix = 'gproxy-tauri-windows'
)
$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Store bundles require a stable version' }
foreach ($arch in @('x86_64', 'aarch64')) {
    $path = "dist/store/$ArtifactPrefix-$arch.msix"
    if (-not (Test-Path $path)) { throw "Missing $arch Store package" }
    $archive = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path $path))
    try {
        $reader = [System.IO.StreamReader]::new($archive.GetEntry('AppxManifest.xml').Open())
        try { [xml]$manifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
        $identity = $manifest.Package.Identity
        $expectedArch = if ($arch -eq 'x86_64') { 'x64' } else { 'arm64' }
        if ($identity.Name -cne $env:MS_STORE_IDENTITY_NAME -or
            $identity.Publisher -cne $env:MS_STORE_IDENTITY_PUBLISHER -or
            $identity.Version -ne "$Version.0" -or
            $identity.ProcessorArchitecture -ne $expectedArch) {
            throw "Store identity, version or architecture mismatch in $path"
        }
    } finally { $archive.Dispose() }
}
if (@(Get-ChildItem dist/store -File).Count -ne 2) { throw 'Expected exactly two architecture packages in dist/store' }
$sdkBin = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$makeappx = Get-ChildItem "$sdkBin/*/x64/makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
if (-not $makeappx) { throw 'Windows SDK makeappx.exe was not found' }
New-Item -ItemType Directory -Force dist/store-upload | Out-Null
& $makeappx.FullName bundle /d dist/store /p dist/store-upload/gproxy-windows.msixbundle /bv "$Version.0" /o
if ($LASTEXITCODE -ne 0) { throw "MSIX bundle packaging failed: $LASTEXITCODE" }
