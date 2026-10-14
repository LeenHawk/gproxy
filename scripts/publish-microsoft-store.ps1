param([Parameter(Mandatory)][string]$Version)

$ErrorActionPreference = 'Stop'
if ($Version -notmatch '^[1-9]\d*\.\d+\.\d+$') { throw 'Store publishing requires a stable version' }
$product = $env:MS_STORE_PRODUCT_ID
if ([string]::IsNullOrWhiteSpace($product)) { throw 'MS_STORE_PRODUCT_ID is required' }
$bundle = 'dist/store-upload/gproxy-windows.msixbundle'
if (-not (Test-Path -LiteralPath $bundle)) { throw 'Windows MSIX bundle is missing' }

$response = & msstore apps get $product
if ($LASTEXITCODE -ne 0) { throw 'Could not read the Store application' }
$app = ($response -join "`n") | ConvertFrom-Json
if ($app.id -ne $product) { throw 'Store application ID mismatch' }
if ($app.packageIdentityName -cne $env:MS_STORE_IDENTITY_NAME -or $app.publisherName -cne $env:MS_STORE_IDENTITY_PUBLISHER) {
    throw 'Partner Center identity does not match the selected Store edition'
}
if (-not $app.lastPublishedApplicationSubmission.id) {
    $message = 'Microsoft Store update skipped: the first submission is not yet published. The existing submission is unchanged.'
    Write-Output "::notice::$message"
    $message | Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY
    exit 0
}
# The official publish command replaces pending submissions; preserve manual work and active reviews.
if ($app.pendingApplicationSubmission.id) {
    throw 'A Store submission is already pending; finish it in Partner Center before retrying this release'
}

$response = & msstore submission get $product
if ($LASTEXITCODE -ne 0) { throw 'Could not read the published Store version' }
$submission = ($response -join "`n") | ConvertFrom-Json
if ($submission.id -ne $app.lastPublishedApplicationSubmission.id) { throw 'Store submission changed during preflight' }
$versions = @($submission.applicationPackages | Where-Object { $_.version -and $_.fileStatus -ne 'PendingDelete' } | ForEach-Object { [version]$_.version })
if ($versions.Count -eq 0) { throw 'Published Store package versions are unavailable' }
if (@($versions | Where-Object { $_ -ge [version]"$Version.0" }).Count -gt 0) {
    "Microsoft Store already has version $Version.0 or newer; skipping duplicate publication." | Write-Output
    exit 0
}

$response = & gh release view "v$Version" --repo $env:GITHUB_REPOSITORY --json 'body,url'
if ($LASTEXITCODE -ne 0) { throw 'Could not read the source release notes' }
$release = ($response -join "`n") | ConvertFrom-Json
$heading = "$env:MS_STORE_DISPLAY_NAME $Version`n`n"
$footer = "`n`n$($release.url)"
$body = [string]$release.body
$maximumBodyLength = 1500 - $heading.Length - $footer.Length
if ($body.Length -gt $maximumBodyLength) { $body = $body.Substring(0, $maximumBodyLength) }
$releaseNotes = $heading + $body + $footer

msstore publish $bundle --appId $product --uploadTimeout 600 --noCommit
if ($LASTEXITCODE -ne 0) { throw 'Microsoft Store package upload failed' }
$response = & msstore submission get $product
if ($LASTEXITCODE -ne 0) { throw 'Could not read the uploaded draft' }
$draft = ($response -join "`n") | ConvertFrom-Json
if ($draft.status -ne 'PendingCommit' -or $draft.id -eq $submission.id) {
    throw 'Expected a new uncommitted Store draft after upload'
}
foreach ($listing in $draft.listings.PSObject.Properties) {
    $listing.Value.baseListing | Add-Member -NotePropertyName releaseNotes -NotePropertyValue $releaseNotes -Force
    if ($listing.Value.platformOverrides) {
        foreach ($platform in $listing.Value.platformOverrides.PSObject.Properties) {
            $platform.Value | Add-Member -NotePropertyName releaseNotes -NotePropertyValue $releaseNotes -Force
        }
    }
}
$metadataPath = Join-Path $env:RUNNER_TEMP "store-$product-metadata.json"
$draft | ConvertTo-Json -Depth 100 | Set-Content -Encoding utf8 $metadataPath
try {
    $response = & msstore submission updateMetadata $product $metadataPath
    if ($LASTEXITCODE -ne 0) { throw 'Could not update the Store release notes; the draft has not been committed' }
} finally { Remove-Item -LiteralPath $metadataPath -ErrorAction SilentlyContinue }
msstore submission publish $product
if ($LASTEXITCODE -ne 0) { throw 'Microsoft Store submission commit failed' }
"$env:MS_STORE_DISPLAY_NAME $Version was submitted to Microsoft Store (product $product). Certification and publication follow Partner Center processing and the existing publishing settings." | Tee-Object -FilePath $env:GITHUB_STEP_SUMMARY -Append
