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

$work = Join-Path ([IO.Path]::GetTempPath()) "gproxy-store-test-$([guid]::NewGuid())"
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

    # Valid trust is necessary but insufficient: the signer must also be Microsoft.
    function Get-AuthenticodeSignature {
        [pscustomobject]@{ Status = $script:status; SignerCertificate = @{ Subject = $script:subject } }
    }
    $script:status = 'Valid'; $script:subject = 'CN=Microsoft Corporation, O=Microsoft Corporation, C=US'
    Assert-StoreSignature $signed
    $script:status = 'HashMismatch'
    Assert-Throws { Assert-StoreSignature $signed } 'Invalid signatures must fail'
    $script:status = 'Valid'; $script:subject = 'CN=Other, O=Other, C=US'
    Assert-Throws { Assert-StoreSignature $signed } 'Other trusted signers must fail'
    $script:subject = 'CN=Microsoft Corporation, O=Microsoft Corporation, C=US'

    # Page through releases; publication date, rather than tag/creation order, wins.
    function Invoke-GhJson([string[]]$Arguments) {
        Assert ($Arguments[0] -eq 'api' -and $Arguments.Count -eq 2) 'gh argument binding'
        if ($Arguments[1].EndsWith('page=1')) {
            1..100 | ForEach-Object { @{ tag_name = "v3.0.$_"; published_at = '2025-01-01'; draft = $false; prerelease = $false } }
        } else {
            @{ tag_name = 'v4.0.0'; published_at = '2026-10-01'; draft = $false; prerelease = $false }
            @{ tag_name = 'nightly'; published_at = '2026-10-02'; draft = $false; prerelease = $false }
            @{ tag_name = 'v5.0.0'; published_at = '2026-10-02'; draft = $false; prerelease = $true }
            @{ tag_name = 'v6.0.0'; published_at = '2026-10-02'; draft = $true; prerelease = $false }
        }
    }
    $recent = @(Get-RecentReleases 'test/repo')
    Assert ($recent.Count -eq 10 -and $recent[0].tag_name -eq 'v4.0.0') 'Select latest ten public stable releases'

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
    # Both architectures must be signed and published before scheduling WinGet.
    $one = @(Sync-Release $release @($edition) $cache (Join-Path $work oneArch))
    Assert ($one.Count -eq 0) 'One architecture must not schedule WinGet'
    $armName = 'gproxy-windows-aarch64.msix'
    $armPath = Join-Path $remote $armName
    New-Package $armPath -Signed -Architecture arm64
    $armHash = (Get-FileHash $armPath).Hash.ToLowerInvariant()
    Add-Content (Join-Path $remote SHA256SUMS) "$armHash  $armName"
    $release.assets += @{ name = $armName }
    $ready = @(Sync-Release $release @($edition) $cache (Join-Path $work ready))
    Assert ($ready.Count -eq 1 -and $ready[0].Edition -eq 'CLI' -and $ready[0].Version -eq '4.0.0') 'Signed pair schedules exact WinGet edition/version'
    $previewReady = @(Sync-Release $release @($edition) $cache (Join-Path $work readyPreview) -Preview)
    Assert ($previewReady.Count -eq 0) 'Dry run must never schedule WinGet'
    Write-Host 'Store MSIX synchronization checks passed.'
} finally {
    $env:GITHUB_REPOSITORY = $savedRepository; $env:MS_STORE_IDENTITY_PUBLISHER = $savedPublisher
    Remove-Item -LiteralPath $work -Recurse -Force
}
