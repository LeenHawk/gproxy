param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$Artifact,
    [switch]$TestSigning
)

$ErrorActionPreference = "Stop"
function Assert-Signature([string]$Path) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    # Temporary onboarding switch; remove after production certificate activation.
    if ($TestSigning) {
        $thumbprint = $env:SIGNPATH_TEST_CERTIFICATE_THUMBPRINT
        if ($thumbprint -notmatch '^[0-9a-fA-F]{40}$' -or -not $signature.SignerCertificate -or
            $signature.SignerCertificate.Thumbprint -ne $thumbprint) {
            throw "Signer does not match the configured test certificate: $Path"
        }
        # An untrusted self-signed root can report UnknownError. Pin its identity,
        # trust it temporarily, then ask Windows to verify the signature again.
        $certificate = $signature.SignerCertificate
        $store = [System.Security.Cryptography.X509Certificates.X509Store]::new('Root', 'CurrentUser')
        $added = $false
        try {
            $store.Open('ReadWrite')
            if ($store.Certificates.Find('FindByThumbprint', $thumbprint, $false).Count -eq 0) {
                $store.Add($certificate)
                $added = $true
            }
            $signature = Get-AuthenticodeSignature -LiteralPath $Path
            if ($signature.Status -ne 'Valid') {
                throw "Invalid test signature: $Path ($($signature.Status): $($signature.StatusMessage))"
            }
        } finally {
            if ($added) { $store.Remove($certificate) }
            $store.Close()
        }
        Write-Warning "Test signing: verified with temporary trust for certificate $thumbprint"
    } elseif ($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate) {
        throw "Missing trusted timestamped signature: $Path ($($signature.Status))"
    }
}

$archive = Join-Path $Directory "$Artifact.zip"
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
try {
    Expand-Archive -LiteralPath $archive -DestinationPath "$work/zip"
    Assert-Signature "$work/zip/gproxy.exe"
    $name = "$Artifact.zip"
    $destination = Join-Path "dist/release" $name
    Copy-Item -LiteralPath $archive -Destination $destination -Force
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLower()
    "$hash  $name" | Out-File -Encoding ascii "$destination.sha256"
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
