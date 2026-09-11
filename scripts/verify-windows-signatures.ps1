param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$Artifact
)

$ErrorActionPreference = "Stop"
function Assert-Signature([string]$Path) {
    $signature = Get-AuthenticodeSignature -LiteralPath $Path
    if ($signature.Status -ne 'Valid' -or -not $signature.TimeStamperCertificate) {
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
