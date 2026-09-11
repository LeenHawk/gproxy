param([Parameter(Mandatory)][string]$Version)

$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^[1-9]\d*\.\d+\.\d+$') { throw 'Store publishing requires a stable version' }
$packageVersion = [version]"$Version.0"
$targets = (Get-Content scripts/release-targets.json -Raw | ConvertFrom-Json).include | Where-Object os -eq windows
$files = @(Get-ChildItem dist/store -File -Filter *.msix)
if ($files.Count -ne @($targets).Count) { throw 'Missing or unexpected Windows MSIX packages' }

foreach ($target in $targets) {
    $file = Join-Path 'dist/store' "$($target.artifact).msix"
    $archive = [System.IO.Compression.ZipFile]::OpenRead((Resolve-Path $file))
    try {
        $entry = $archive.GetEntry('AppxManifest.xml')
        if (-not $entry) { throw "Missing AppxManifest.xml in $file" }
        $reader = [System.IO.StreamReader]::new($entry.Open())
        try { [xml]$manifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
        $identity = $manifest.Package.Identity
        $architecture = if ($target.target.StartsWith('aarch64-')) { 'arm64' } else { 'x64' }
        if ($identity.Name -cne $env:MS_STORE_IDENTITY_NAME -or
            $identity.Publisher -cne $env:MS_STORE_IDENTITY_PUBLISHER -or
            [version]$identity.Version -ne $packageVersion -or
            $identity.ProcessorArchitecture -ne $architecture) {
            throw "Store identity, version or architecture mismatch in $file"
        }
    } finally { $archive.Dispose() }
}

$sdk = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits/10/bin'
$makeAppx = Get-ChildItem "$sdk/*/x64/makeappx.exe" |
    Sort-Object { [version]$_.Directory.Parent.Name } -Descending | Select-Object -First 1
if (-not $makeAppx) { throw 'Windows SDK MakeAppx was not found' }
New-Item -ItemType Directory -Force -Path dist/store-upload | Out-Null
& $makeAppx.FullName bundle /d (Resolve-Path dist/store) /p "$PWD/dist/store-upload/gproxy-windows.msixbundle" /bv "$Version.0" /o
if ($LASTEXITCODE -ne 0) { throw "MSIX bundle validation failed: $LASTEXITCODE" }
