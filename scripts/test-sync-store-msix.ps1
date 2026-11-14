# Offline contract checks; Windows Authenticode itself is exercised by the sync job.
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/sync-store-msix.ps1"

function Assert($Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}
function Assert-Throws([scriptblock]$Action, [string]$Message) {
    $failed = $false
    try { & $Action } catch { $failed = $true }
    Assert $failed $Message
}
function New-Package([string]$Path, [string]$Payload = 'release binary', [switch]$Signed, [string]$Architecture = 'x64') {
    $archive = [IO.Compression.ZipFile]::Open($Path, 'Create')
    try {
        $files = @{
            'AppxManifest.xml' = '<Package><Identity Name="Test.CLI" Publisher="CN=Test" Version="4.0.0.0" ProcessorArchitecture="ARCH" /></Package>'.Replace('ARCH', $Architecture)
            'gproxy.exe' = $Payload
        }
        if ($Signed) { $files['AppxSignature.p7x'] = 'fixture signature' }
        foreach ($entry in $files.GetEnumerator()) {
            $writer = [IO.StreamWriter]::new($archive.CreateEntry($entry.Key).Open())
            try { $writer.Write($entry.Value) } finally { $writer.Dispose() }
        }
    } finally { $archive.Dispose() }
}

$work = Join-Path (Join-Path $PSScriptRoot ../target) "gproxy-store-test-$([guid]::NewGuid())"
New-Item -ItemType Directory $work | Out-Null
$savedRepository = $env:GITHUB_REPOSITORY
$savedPublisher = $env:MS_STORE_IDENTITY_PUBLISHER
try {
    $unsigned = Join-Path $work unsigned.msix
    $signed = Join-Path $work signed.msix
    $changed = Join-Path $work changed.msix
    New-Package $unsigned
    New-Package $signed -Signed
    New-Package $changed 'different release binary' -Signed
    $info = Get-PackageInfo $unsigned
    $expected = @{ Name = 'Test.CLI'; Publisher = 'CN=Test'; Version = '4.0.0.0'; Architecture = 'x64' }
    Assert (Test-PackageMatch $info $expected) 'Exact package should match'
    foreach ($field in @('Name', 'Publisher', 'Version', 'Architecture')) {
        $wrong = $expected.Clone(); $wrong[$field] = 'wrong'
        Assert (-not (Test-PackageMatch $info $wrong)) "Must reject mismatched $field"
    }
    Assert (-not $info.Signed) 'Unsigned package detection'
    Assert (Get-PackageInfo $signed).Signed 'Signed package detection'
    Assert-SamePayload $unsigned $signed
    Assert-Throws { Assert-SamePayload $unsigned $changed } 'Changed binaries must be rejected'

    # Valid trust is necessary but insufficient: the Store signs as the publisher
    # with a certificate issued by a Microsoft Marketplace CA.
    $env:MS_STORE_IDENTITY_PUBLISHER = 'CN=Test'
    $marketplace = 'CN=Microsoft Marketplace CA G 027, OU=EOC, O=Microsoft Corporation, L=Redmond, S=Washington, C=US'
    function Get-AuthenticodeSignature {
        [pscustomobject]@{ Status = $script:status; SignerCertificate = @{ Subject = $script:subject; Issuer = $script:issuer } }
    }
    $script:status = 'Valid'; $script:subject = 'CN=Test'; $script:issuer = $marketplace
    Assert-StoreSignature $signed
    $script:status = 'HashMismatch'
    Assert-Throws { Assert-StoreSignature $signed } 'Invalid signatures must fail'
    $script:status = 'Valid'; $script:subject = 'CN=Other'
    Assert-Throws { Assert-StoreSignature $signed } 'Other publishers must fail'
    $script:subject = 'CN=Test'; $script:issuer = 'CN=Other CA, O=Other, C=US'
    Assert-Throws { Assert-StoreSignature $signed } 'Other issuers must fail'
    $script:issuer = $marketplace

    # Follow package-level catalog fulfillment and the public SFS API contract.
    $script:storeUnavailable = $false
    $script:storeCalls = @()
    function Invoke-StoreJson([string]$Uri, [string]$Method = 'Get', [string]$Body) {
        $script:storeCalls += $Uri
        if ($Uri -like 'https://displaycatalog.mp.microsoft.com/*') {
            @{ Product = @{ ProductId = 'PRODUCT'; DisplaySkuAvailabilities = @(
                @{ Sku = @{ Properties = @{ Packages = @(
                    @{ FulfillmentData = @{ WuCategoryId = 'category' } },
                    @{ FulfillmentData = '{"WuCategoryId":"category"}' }
                ) } } }
            ) } }
        } elseif ($Uri.EndsWith('/category/versions/latest?action=select')) {
            Assert ($Method -eq 'Post' -and $Body -eq '{"TargetingAttributes":{}}') 'SFS version request'
            if (-not $script:storeUnavailable) { @{ ContentId = @{ Version = 'delivery-version' } } }
        } elseif ($Uri.EndsWith('/category/versions/delivery-version/files?action=GenerateDownloadInfo')) {
            Assert ($Method -eq 'Post') 'SFS file request'
            @{ FileId = 'package.msixbundle' }
            @{ FileId = 'package.eappxbundle' }
        } else { throw "Unexpected Store URL: $Uri" }
    }
    $files = @(Get-StoreFiles 'PRODUCT')
    Assert ($files.Count -eq 1 -and $files[0].FileId -eq 'package.msixbundle') 'Exclude encrypted packages'
    Assert ($script:storeCalls.Count -eq 3) 'Deduplicate catalog categories'
    $script:storeUnavailable = $true
    Assert (@(Get-StoreFiles 'PRODUCT').Count -eq 0) 'No SFS delivery is a safe no-op'

    # Only GitHub's latest public stable release can be mutated.
    function Invoke-GhJson([string[]]$Arguments) {
        Assert (($Arguments -join ' ') -eq 'api repos/test/repo/releases/latest') 'Use latest stable release endpoint'
        $script:latest
    }
    $script:latest = @{ tag_name = 'v4.0.0'; draft = $false; prerelease = $false }
    Assert ((Get-LatestRelease 'test/repo').tag_name -eq 'v4.0.0') 'Select latest stable release'
    foreach ($bad in @(
        @{ tag_name = 'nightly' }, @{ tag_name = 'staging' },
        @{ tag_name = 'v5.0.0'; prerelease = $true }, @{ tag_name = 'v5.0.0'; draft = $true }
    )) {
        $script:latest = $bad
        Assert-Throws { Get-LatestRelease 'test/repo' } 'Reject draft, prerelease and update channels'
    }

    $env:GITHUB_REPOSITORY = 'test/repo'; $env:MS_STORE_IDENTITY_PUBLISHER = 'CN=Test'
    $name = 'gproxy-windows-x86_64.msix'
    $remote = Join-Path $work remote
    New-Item -ItemType Directory $remote | Out-Null
    Copy-Item $unsigned (Join-Path $remote $name)
    $oldHash = (Get-FileHash $unsigned).Hash.ToLowerInvariant()
    $newHash = (Get-FileHash $signed).Hash.ToLowerInvariant()
    $otherLine = ('a' * 64) + '  unrelated.zip'
    @("$oldHash  $name", $otherLine) | Set-Content (Join-Path $remote SHA256SUMS)
    $script:uploads = @()
    function gh {
        $global:LASTEXITCODE = 0
        if ($args[1] -eq 'download') {
            $pattern = $args[[array]::IndexOf($args, '--pattern') + 1]
            $destination = $args[[array]::IndexOf($args, '--dir') + 1]
            Copy-Item (Join-Path $remote $pattern) $destination -Force
        } elseif ($args[1] -eq 'upload') {
            $script:uploads += [IO.Path]::GetFileName($args[3])
            Copy-Item $args[3] $remote -Force
        } else { throw 'Unexpected gh call' }
    }
    $release = @{ tag_name = 'v4.0.0'; assets = @(@{ name = $name }) }
    $edition = @{ Identity = 'Test.CLI'; Prefix = 'gproxy-windows'; Product = 'PRODUCT' }
    $cache = @{ PRODUCT = @((Get-PackageInfo $signed)) }

    Sync-Release $release @($edition) @{ PRODUCT = @() } (Join-Path $work unavailable)
    Assert ($script:uploads.Count -eq 0) 'Unavailable Store version must not upload'
    Sync-Release $release @($edition) $cache (Join-Path $work preview) -Preview
    Assert ($script:uploads.Count -eq 0) 'Dry run must not upload'
    Assert-Throws {
        Sync-Release $release @($edition) @{ PRODUCT = @((Get-PackageInfo $changed)) } (Join-Path $work changedPayload)
    } 'Changed Store payload must not replace release asset'
    Assert ($script:uploads.Count -eq 0) 'Changed payload must not upload'
    @("$('0' * 64)  $name", $otherLine) | Set-Content (Join-Path $remote SHA256SUMS)
    Assert-Throws {
        Sync-Release $release @($edition) $cache (Join-Path $work badChecksum)
    } 'Corrupt original release must not be replaced'
    Assert ($script:uploads.Count -eq 0) 'Corrupt original must not upload'
    @("$oldHash  $name", $otherLine) | Set-Content (Join-Path $remote SHA256SUMS)
    Sync-Release $release @($edition) $cache (Join-Path $work replace)
    Assert (($script:uploads -join ',') -eq "$name,SHA256SUMS") 'Replace only package and checksums'
    $sums = @(Get-Content (Join-Path $remote SHA256SUMS))
    Assert ($sums[0] -eq "$newHash  $name" -and $sums[1] -eq $otherLine) 'Update only matching checksum'
    $script:uploads = @()
    Sync-Release $release @($edition) $cache (Join-Path $work repeat)
    Assert ($script:uploads.Count -eq 0) 'Already signed package must be idempotent'
    @("$oldHash  $name", $otherLine) | Set-Content (Join-Path $remote SHA256SUMS)
    Sync-Release $release @($edition) $cache (Join-Path $work repair)
    Assert (($script:uploads -join ',') -eq 'SHA256SUMS') 'Repair interrupted checksum upload without replacing signed package'
    # Current Headless and Desktop filenames must use the same checked replacement path.
    foreach ($prefix in @('gproxy-headless-windows', 'gproxy-tauri-windows')) {
        $headlessName = "$prefix-aarch64.msix"
        $headlessOriginal = Join-Path $remote $headlessName
        $headlessSigned = Join-Path $work "$prefix-signed.msix"
        New-Package $headlessOriginal -Architecture arm64
        New-Package $headlessSigned -Signed -Architecture arm64
        $headlessHash = (Get-FileHash $headlessOriginal).Hash.ToLowerInvariant()
        Add-Content (Join-Path $remote SHA256SUMS) "$headlessHash  $headlessName"
        $script:uploads = @()
        $edition.Prefix = $prefix
        $release.assets = @(@{ name = $headlessName })
        Sync-Release $release @($edition) @{ PRODUCT = @((Get-PackageInfo $headlessSigned)) } (Join-Path $work $prefix)
        Assert (($script:uploads -join ',') -eq "$headlessName,SHA256SUMS") "Replace $prefix ARM64 package"
    }
    Write-Host 'Store MSIX synchronization checks passed.'
} finally {
    $env:GITHUB_REPOSITORY = $savedRepository; $env:MS_STORE_IDENTITY_PUBLISHER = $savedPublisher
    Remove-Item -LiteralPath $work -Recurse -Force
}
