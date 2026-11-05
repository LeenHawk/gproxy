param(
    [Parameter(Mandatory)][string]$Directory,
    [Parameter(Mandatory)][string]$Artifact,
    [string]$OutputDirectory = 'dist/release'
)

$ErrorActionPreference = "Stop"
. "$PSScriptRoot/assert-windows-signature.ps1"
function Assert-Signature([string]$Path) {
    Assert-WindowsSignature $Path | Out-Null
}

$archive = Join-Path $Directory "$Artifact.zip"
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
try {
    Expand-Archive -LiteralPath $archive -DestinationPath "$work/zip"
    Assert-Signature "$work/zip/gproxy.exe"
    $name = "$Artifact.zip"
    $destination = Join-Path $OutputDirectory $name
    New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
    Copy-Item -LiteralPath $archive -Destination $destination -Force
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLower()
    "$hash  $name" | Out-File -Encoding ascii "$destination.sha256"
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
