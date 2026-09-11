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
if (-not $app.lastPublishedApplicationSubmission.id) {
    throw 'Complete the first Store submission in Partner Center before enabling automatic updates'
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

msstore publish $bundle --appId $product --uploadTimeout 600
if ($LASTEXITCODE -ne 0) { throw 'Microsoft Store upload or submission commit failed' }
"GPROXY $Version was submitted to Microsoft Store. Certification and publication follow Partner Center processing and the existing publishing settings." | Tee-Object -FilePath $env:GITHUB_STEP_SUMMARY -Append
