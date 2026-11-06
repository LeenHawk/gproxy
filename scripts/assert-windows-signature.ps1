# Dot-sourced by the Windows signature verifiers: a trusted, timestamped
# Authenticode signature is required.
function Assert-WindowsSignature([string]$Path) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate) {
        throw "Missing trusted timestamped signature: $Path ($($signature.Status))"
    }
    $signature
}
