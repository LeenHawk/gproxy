param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$Artifact,
    [Parameter(Mandatory)][string]$IdentityName,
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Publisher
)

$ErrorActionPreference = 'Stop'
function Assert-Signature([string]$Path) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate) {
        throw "Missing trusted timestamped signature: $Path ($($signature.Status))"
    }
    if ($signature.SignerCertificate.Subject -cne $Publisher) {
        throw "Signing certificate does not match the configured MSIX publisher: $Path"
    }
}

$package = Join-Path $Directory "$Artifact.msix"
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
try {
    Assert-Signature $package
    [System.IO.Compression.ZipFile]::ExtractToDirectory($package, $work)
    [xml]$manifest = Get-Content -LiteralPath "$work/AppxManifest.xml" -Raw
    if ($manifest.Package.Identity.Name -cne $IdentityName -or $manifest.Package.Identity.Publisher -cne $Publisher) {
        throw 'Signed MSIX identity does not match the configured release identity'
    }
    Assert-Signature (Join-Path $work $Executable)
    $name = "$Artifact.msix"
    $destination = Join-Path 'dist/release' $name
    Copy-Item -LiteralPath $package -Destination $destination -Force
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLower()
    "$hash  $name" | Set-Content -Encoding ascii "$destination.sha256"
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
