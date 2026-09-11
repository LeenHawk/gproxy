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
$msi = (Resolve-Path (Join-Path $Directory "$Artifact.msi")).Path
$work = Join-Path ([System.IO.Path]::GetTempPath()) ([System.Guid]::NewGuid())
try {
    Assert-Signature $msi
    Expand-Archive -LiteralPath $archive -DestinationPath "$work/zip"
    Assert-Signature "$work/zip/gproxy.exe"
    $arguments = "/a `"$msi`" /qn TARGETDIR=`"$work/msi`""
    $process = Start-Process msiexec.exe -ArgumentList $arguments -Wait -PassThru
    if ($process.ExitCode -ne 0) { throw "MSI extraction failed: $($process.ExitCode)" }
    foreach ($name in @('gproxy.exe', 'GPROXY.ps1', 'GPROXY.vbs')) {
        $files = @(Get-ChildItem "$work/msi" -Recurse -File -Filter $name)
        if ($files.Count -ne 1) { throw "Expected one $name in signed MSI, found $($files.Count)" }
        Assert-Signature $files[0].FullName
    }
    foreach ($extension in @('zip', 'msi')) {
        $name = "$Artifact.$extension"
        $destination = Join-Path "dist/release" $name
        Copy-Item -LiteralPath (Join-Path $Directory $name) -Destination $destination -Force
        $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLower()
        "$hash  $name" | Out-File -Encoding ascii "$destination.sha256"
    }
} finally {
    Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
