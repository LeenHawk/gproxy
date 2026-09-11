---
title: Code signing policy
description: "GPROXY Windows signing status, PR review, privacy, and signature verification."
---

## Status

GPROXY has applied to SignPath Foundation and is awaiting review. Production
Windows signing is not enabled yet. Neither this policy nor the integration
implies that the application has been accepted or that existing assets are signed.

After acceptance and activation, Windows tagged releases will use:
**Free code signing provided by [SignPath.io](https://signpath.io), certificate
by [SignPath Foundation](https://signpath.org).**

## Scope and review

The project repository is [LeenHawk/gproxy](https://github.com/LeenHawk/gproxy).
Only artifacts built by its release workflow from this repository may be signed.
The signing configuration covers both Windows x86_64 and ARM64:

- The `gproxy.exe` inside the portable ZIP.
- The MSI installer, its `gproxy.exe`, and its PowerShell and VBScript launchers.

Committer and reviewer: [LeenHawk](https://github.com/LeenHawk).
Release changes are reviewed through GitHub pull requests before merging into
protected `main`. Release tags are cut from reviewed commits; CI automatically
submits, downloads and verifies signed packages using an automatic signing policy.
Signing participants must use multi-factor authentication for GitHub and SignPath.

Once enabled, tagged stable and prerelease builds must obtain trusted,
timestamped signatures before publication. Failed, rejected, or timed-out
requests stop the release. Continuous `staging` builds remain unsigned.
Windows x86_64 release binaries are packed with UPX before signing; Windows
ARM64 binaries remain unpacked.

## Privacy

GPROXY is a self-hosted gateway. Requests and any attached content are sent to
the upstream providers configured by the operator. OAuth authorization and
credential refresh contact the corresponding provider. The operator controls
database storage, usage records, request logging, retention, and access.
Provider and deployment-service privacy policies also apply to those services.

Update checks and downloads contact GitHub by default. Opening Console can
cause the gateway to retrieve the signed announcement feed from
`gproxy.leenhawk.com`. These requests expose ordinary connection information,
such as the source IP and User-Agent, to those endpoints and their hosting
providers; the announcement User-Agent includes the GPROXY version. They do
not attach inference request bodies or upstream credentials. An outbound proxy
changes which source IP the destination sees. We do not claim that the program
makes no network connections until the user explicitly requests each one.

Windows setup asks whether to enable autostart. Uninstall the MSI through
Windows Settings; review and remove retained user data separately if it is no
longer needed. Portable installations are removed by stopping the process and
deleting the extracted files; retain or delete the configured data directory
as appropriate.

## Verify a download

Extract the portable archive, then inspect the EXE or MSI in PowerShell:

```powershell
Get-AuthenticodeSignature .\gproxy.exe |
  Format-List Status, StatusMessage, SignerCertificate, TimeStamperCertificate
```

For production-signed assets, expect `Status: Valid`, a SignPath Foundation
publisher certificate, and a timestamp certificate. Check the actual file;
the release date or filename alone is not proof of signing. Authenticode does
not guarantee the absence of antivirus false positives or SmartScreen warnings.

The Ed25519 signature on the update manifest is a separate mechanism. It lets
GPROXY verify updates but does not establish Windows publisher trust.
