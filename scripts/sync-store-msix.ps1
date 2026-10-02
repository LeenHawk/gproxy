param([switch]$DryRun)

$ErrorActionPreference = 'Stop'

function Write-Summary([string]$Message) {
    Write-Host $Message
    if ($env:GITHUB_STEP_SUMMARY) { Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value $Message }
}

function Invoke-GhJson([string[]]$Arguments) {
    $result = & gh @Arguments
    if ($LASTEXITCODE -ne 0) { throw "gh failed: $($Arguments -join ' ')" }
    ($result -join "`n") | ConvertFrom-Json
}

function Get-PackageInfo([string]$Path) {
    $archive = [IO.Compression.ZipFile]::OpenRead($Path)
    try {
        $entry = $archive.GetEntry('AppxManifest.xml')
        if (-not $entry) { throw "Missing AppxManifest.xml: $Path" }
        $reader = [IO.StreamReader]::new($entry.Open())
        try { [xml]$manifest = $reader.ReadToEnd() } finally { $reader.Dispose() }
        $identity = $manifest.Package.Identity
        [pscustomobject]@{
            Path = $Path
            Name = [string]$identity.Name
            Publisher = [string]$identity.Publisher
            Version = [string]$identity.Version
            Architecture = [string]$identity.ProcessorArchitecture
            Signed = $null -ne $archive.GetEntry('AppxSignature.p7x')
        }
    } finally { $archive.Dispose() }
}

function Assert-StoreSignature([string]$Path) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch '(?:^|,\s*)O=Microsoft Corporation(?:,|$)') {
        throw "Package does not have a trusted Microsoft signature: $Path ($($signature.Status))"
    }
}

function Test-PackageMatch($Package, $Expected) {
    $Package.Name -ceq $Expected.Name -and $Package.Publisher -ceq $Expected.Publisher -and
        $Package.Version -ceq $Expected.Version -and $Package.Architecture -ceq $Expected.Architecture
}

function Assert-SamePayload([string]$Original, [string]$Signed) {
    $before = [IO.Compression.ZipFile]::OpenRead($Original)
    $after = [IO.Compression.ZipFile]::OpenRead($Signed)
    try {
        # Store certification can change the manifest and signing metadata.
        # Every application file must still be exactly the released payload.
        $exclude = '^(AppxManifest\.xml|AppxBlockMap\.xml|AppxSignature\.p7x|\[Content_Types\]\.xml|AppxMetadata/.*)$'
        $originalFiles = @($before.Entries | Where-Object { $_.Name -and $_.FullName -notmatch $exclude })
        $signedFiles = @($after.Entries | Where-Object { $_.Name -and $_.FullName -notmatch $exclude })
        if ($originalFiles.Count -ne $signedFiles.Count) { throw 'Store package payload file list differs from the release' }
        foreach ($entry in $originalFiles) {
            $other = $after.GetEntry($entry.FullName)
            if (-not $other) { throw "Store package is missing $($entry.FullName)" }
            $left = $entry.Open()
            $right = $other.Open()
            $sha = [Security.Cryptography.SHA256]::Create()
            try {
                if ([Convert]::ToHexString($sha.ComputeHash($left)) -cne [Convert]::ToHexString($sha.ComputeHash($right))) {
                    throw "Store package payload differs from the release: $($entry.FullName)"
                }
            } finally { $left.Dispose(); $right.Dispose(); $sha.Dispose() }
        }
    } finally { $before.Dispose(); $after.Dispose() }
}

function Invoke-StoreJson([string]$Uri, [string]$Method = 'Get', [string]$Body) {
    $options = @{ Uri = $Uri; Method = $Method; TimeoutSec = 120 }
    if ($Method -eq 'Post') { $options.ContentType = 'application/json'; $options.Body = $Body }
    try { Invoke-RestMethod @options } catch {
        # A product or its delivery files may not yet be publicly available.
        if ($_.Exception.Response.StatusCode -eq 404) { return $null }
        throw
    }
}

function Get-StoreFiles([string]$ProductId) {
    # Public Store catalog and the same SFS delivery API used by microsoft/winget-cli.
    # The consumer catalog does not require interactive Entra authentication.
    $catalog = Invoke-StoreJson "https://displaycatalog.mp.microsoft.com/v7.0/products/${ProductId}?market=US&languages=en-us"
    if (-not $catalog) { Write-Summary "$ProductId is not in the public Store catalog yet."; return }
    if ($catalog.Product.ProductId -cne $ProductId) { throw 'Store product ID mismatch' }
    $categories = @($catalog.Product.DisplaySkuAvailabilities | ForEach-Object {
        $fulfillment = $_.Sku.Properties.FulfillmentData
        if ($fulfillment -is [string]) { $fulfillment = $fulfillment | ConvertFrom-Json }
        if ($fulfillment.WuCategoryId) { $fulfillment.WuCategoryId }
    } | Sort-Object -Unique)
    foreach ($category in $categories) {
        $base = 'https://storeapps.api.cdp.microsoft.com/api/v2/contents/storeapps/namespaces/default/names/' +
            [Uri]::EscapeDataString($category) + '/versions/'
        $latest = Invoke-StoreJson "${base}latest?action=select" Post '{"TargetingAttributes":{}}'
        if (-not $latest) { Write-Summary "$ProductId has no public SFS delivery yet."; continue }
        $version = [Uri]::EscapeDataString($latest.ContentId.Version)
        $files = Invoke-StoreJson "${base}${version}/files?action=GenerateDownloadInfo" Post ''
        $files | Where-Object { $_.FileId -match '\.(msix|appx|msixbundle|appxbundle)$' }
    }
}

function Get-SignedPackages($Edition, [string]$Directory) {
    New-Item -ItemType Directory -Force $Directory | Out-Null
    $index = 0
    foreach ($file in @(Get-StoreFiles $Edition.Product)) {
        # FileMoniker carries the identity/version even when FileId is a GUID.
        if (-not $file.FileMoniker.StartsWith($Edition.Identity + '_', [StringComparison]::Ordinal)) { continue }
        if ($file.FileMoniker.Split('_')[1] -notin $Edition.Versions) { continue }
        $index++
        $extension = [IO.Path]::GetExtension($file.FileId)
        $path = Join-Path $Directory "$index$extension"
        Invoke-WebRequest -Uri $file.Url -OutFile $path -TimeoutSec 300
        $expectedHash = [Convert]::ToHexString([Convert]::FromBase64String($file.Hashes.Sha256))
        if ((Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash -cne $expectedHash) { throw 'Store download checksum mismatch' }
        Assert-StoreSignature $path
        if ($extension -in @('.msixbundle', '.appxbundle')) {
            $bundle = [IO.Compression.ZipFile]::OpenRead($path)
            try {
                foreach ($entry in $bundle.Entries) {
                    if ($entry.FullName -notmatch '\.(msix|appx)$') { continue }
                    # Preserve the signed inner archive bytes, never repack them.
                    $inner = Join-Path $Directory "$index-$([IO.Path]::GetFileName($entry.FullName))"
                    [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $inner, $true)
                    Assert-StoreSignature $inner
                    Get-PackageInfo $inner
                }
            } finally { $bundle.Dispose() }
        } else { Get-PackageInfo $path }
    }
}

function Get-RecentReleases([string]$Repository) {
    $releases = @()
    $page = 1
    do {
        $batch = @(Invoke-GhJson -Arguments @('api', "repos/$Repository/releases?per_page=100&page=$page"))
        $releases += @($batch | Where-Object { -not $_.draft -and -not $_.prerelease -and $_.tag_name -match '^v[1-9]\d*\.\d+\.\d+$' })
        $page++
    } while ($batch.Count -eq 100)
    $releases | Sort-Object published_at -Descending | Select-Object -First 10
}

function Sync-Release($Release, $Editions, [hashtable]$StoreCache, [string]$Work, [switch]$Preview) {
    $tag = $Release.tag_name
    $directory = Join-Path $Work $tag
    New-Item -ItemType Directory -Force $directory | Out-Null
    $assets = @($Release.assets | Where-Object { $_.name -match '^gproxy-(tauri-)?windows-(x86_64|aarch64)\.msix$' })
    if (-not $assets.Count) { Write-Summary "${tag}: no MSIX assets."; return }
    gh release download $tag --repo $env:GITHUB_REPOSITORY --pattern SHA256SUMS --dir $directory
    if ($LASTEXITCODE -ne 0) { throw "Could not download $tag checksums" }
    $checksumPath = Join-Path $directory SHA256SUMS
    $lines = @(Get-Content -LiteralPath $checksumPath)
    foreach ($asset in $assets) {
        $name = $asset.name
        $edition = $Editions | Where-Object { $name.StartsWith($_.Prefix + '-', [StringComparison]::Ordinal) }
        $arch = if ($name.EndsWith('-x86_64.msix')) { 'x64' } else { 'arm64' }
        $expected = @{ Name = $edition.Identity; Publisher = $env:MS_STORE_IDENTITY_PUBLISHER; Version = $tag.Substring(1) + '.0'; Architecture = $arch }
        $pattern = '^([a-fA-F0-9]{64})\s+\*?' + [regex]::Escape($name) + '$'
        $positions = @(for ($i = 0; $i -lt $lines.Count; $i++) { if ($lines[$i] -match $pattern) { $i } })
        if ($positions.Count -ne 1) { throw "Expected exactly one checksum for $tag/$name" }
        gh release download $tag --repo $env:GITHUB_REPOSITORY --pattern $name --dir $directory
        if ($LASTEXITCODE -ne 0) { throw "Could not download $tag/$name" }
        $original = Join-Path $directory $name
        $info = Get-PackageInfo $original
        if (-not (Test-PackageMatch $info $expected)) {
            Write-Summary "${tag}/${name}: different Store identity or version; unchanged."
            continue
        }
        $hash = (Get-FileHash -LiteralPath $original -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($info.Signed) {
            Assert-StoreSignature $original
            Write-Summary "${tag}/${name}: already Microsoft-signed."
            # Also repairs a checksum upload interrupted after replacing the MSIX.
        } else {
            if ($lines[$positions[0]].Substring(0, 64).ToLowerInvariant() -cne $hash) { throw "Release checksum mismatch: $tag/$name" }
            if (-not $StoreCache.ContainsKey($edition.Product)) {
                $StoreCache[$edition.Product] = @(Get-SignedPackages $edition (Join-Path $Work $edition.Product))
            }
            $matches = @($StoreCache[$edition.Product] | Where-Object { Test-PackageMatch $_ $expected })
            if (-not $matches.Count) { Write-Summary "${tag}/${name}: matching signed Store package not available yet."; continue }
            $candidate = $matches[0]
            Assert-SamePayload $original $candidate.Path
            $hash = (Get-FileHash -LiteralPath $candidate.Path -Algorithm SHA256).Hash.ToLowerInvariant()
            if (-not $Preview) {
                Copy-Item -LiteralPath $candidate.Path -Destination $original -Force
                gh release upload $tag $original --repo $env:GITHUB_REPOSITORY --clobber
                if ($LASTEXITCODE -ne 0) { throw "Could not replace $tag/$name" }
            }
            Write-Summary "${tag}/${name}: $(if ($Preview) { 'would replace with' } else { 'replaced with' }) verified Microsoft-signed package."
        }
        if ($lines[$positions[0]].Substring(0, 64).ToLowerInvariant() -cne $hash) {
            $lines[$positions[0]] = "$hash  $name"
            if (-not $Preview) {
                $lines | Set-Content -LiteralPath $checksumPath -Encoding ascii
                gh release upload $tag $checksumPath --repo $env:GITHUB_REPOSITORY --clobber
                if ($LASTEXITCODE -ne 0) { throw "Could not update $tag checksums" }
            }
        }
    }
}

function Main {
    foreach ($name in @('GITHUB_REPOSITORY', 'MS_STORE_PRODUCT_ID', 'MS_STORE_CLI_PRODUCT_ID', 'MS_STORE_IDENTITY_NAME', 'MS_STORE_CLI_IDENTITY_NAME', 'MS_STORE_IDENTITY_PUBLISHER')) {
        if ([string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($name))) { throw "Missing $name" }
    }
    $editions = @(
        @{ Product = $env:MS_STORE_PRODUCT_ID; Identity = $env:MS_STORE_IDENTITY_NAME; Prefix = 'gproxy-tauri-windows' },
        @{ Product = $env:MS_STORE_CLI_PRODUCT_ID; Identity = $env:MS_STORE_CLI_IDENTITY_NAME; Prefix = 'gproxy-windows' }
    )
    $work = Join-Path ([IO.Path]::GetTempPath()) "gproxy-store-sync-$([guid]::NewGuid())"
    $cache = @{}
    $failures = @()
    try {
        $releases = @(Get-RecentReleases $env:GITHUB_REPOSITORY)
        foreach ($edition in $editions) { $edition.Versions = @($releases | ForEach-Object { $_.tag_name.Substring(1) + '.0' }) }
        foreach ($release in $releases) {
            try { Sync-Release $release $editions $cache $work -Preview:$DryRun } catch {
                $failures += "$($release.tag_name): $($_.Exception.Message)"
                Write-Summary "::error::$($failures[-1])"
            }
        }
        if ($failures.Count) { throw "$($failures.Count) release(s) failed Store synchronization" }
    } finally { if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force } }
}

if ($MyInvocation.InvocationName -ne '.') { Main }
