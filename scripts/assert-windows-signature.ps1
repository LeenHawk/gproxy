# Dot-sourced by the Windows signature verifiers.
# With $env:SIGNPATH_TEST_CERTIFICATE_THUMBPRINT set (test-signing policy only),
# the signer must be that exact certificate; it is trusted temporarily for the
# check and the timestamp requirement is relaxed. Otherwise a trusted,
# timestamped signature is required.
function Assert-WindowsSignature([string]$Path) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    $thumbprint = $env:SIGNPATH_TEST_CERTIFICATE_THUMBPRINT
    if (-not $thumbprint) {
        if ($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate) {
            throw "Missing trusted timestamped signature: $Path ($($signature.Status))"
        }
        return $signature
    }
    if ($thumbprint -notmatch '^[0-9a-fA-F]{40}$' -or -not $signature.SignerCertificate -or
        $signature.SignerCertificate.Thumbprint -ne $thumbprint) {
        throw "Signer does not match the configured test certificate: $Path"
    }
    # An untrusted self-signed root reports UnknownError. Trust the pinned
    # certificate for this check only, then let Windows verify again.
    # LocalMachine (runners are elevated): adding to CurrentUser\Root opens a
    # confirmation dialog that blocks a non-interactive runner forever.
    $certificate = $signature.SignerCertificate
    $store = [System.Security.Cryptography.X509Certificates.X509Store]::new('Root', 'LocalMachine')
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
    $signature
}
