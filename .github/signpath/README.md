# SignPath activation

The Foundation has confirmed that the project is eligible for the OSS program.
Account onboarding and technical validation with the self-signed test certificate
come next; the production certificate will be ordered after SignPath reviews the
working setup. Keep `SIGNPATH_ENABLED` unset or `false` until that is complete.

## Review material

- Repository: https://github.com/LeenHawk/gproxy
- Downloads: https://gproxy.leenhawk.com/getting-started/downloads/
- Code signing policy: https://gproxy.leenhawk.com/deployment/code-signing/
- Maintainer/PR reviewer: https://github.com/LeenHawk
- License: AGPL-3.0-or-later application, MIT for the crates identified in
  their manifests; no commercial dual-licensing is introduced by this integration.
- Scope: x86_64 and ARM64 portable CLI EXEs and release CLI/Desktop MSIX packages.
  Microsoft Store submissions retain their Partner Center identity.
- Build: GitHub-hosted Windows runners, public source and release workflow,
  embedded Console compiled by the same workflow. Windows x86_64 and ARM64
  binaries use UPX before signing.

The policy pages must be deployed before sharing their public URLs with the
reviewer. Confirm GitHub and SignPath MFA, repository permissions, privacy
disclosures, and installer behavior against the Foundation's
[terms](https://signpath.org/terms). Do not mark those checks complete merely
because the policy describes them.

## Account onboarding

1. Create your SignPath account using the invitation email.
2. Accept the invitation to the OSS organization with that account.
3. Only then confirm the CI user's email address.

## SignPath configuration

Open the organization linked in your invitation, then configure:

- **Trusted Build Systems**: connect GitHub and authorize `LeenHawk/gproxy`.
  Use `.github/workflows/release.yml`; allow both `main` and `dev` branches
  in origin verification and permit intended version tag (`v*`) builds.
  See the [official GitHub integration guide](https://about.signpath.io/documentation/trusted-build-systems/github).
- **Projects**: create/select the GPROXY project and record its project slug.
- **Artifact Configurations**: add slug **`windows-release`**, paste
  [windows-release.xml](windows-release.xml) as its XML configuration.
  The two ZIP layers represent GitHub's artifact envelope and the portable ZIP;
  CI already creates and uploads them. `gproxy*.zip` matches the portable archive.
- Add a second artifact configuration with slug **`windows-msix`**, using
  [windows-msix.xml](windows-msix.xml). It signs the application EXE inside the
  MSIX and then the outer MSIX, matching `gproxy*.msix` and `gproxy*.exe`.
  Allow both artifact configurations in the signing policy.
- **Signing Policies**: select the production certificate when available,
  enable automatic signing without an additional approval process, and grant
  the CI user permission to submit requests. Record the signing policy slug.
- **CI user API token**: create a token for that submitter and store it directly
  in GitHub as described below. Record the organization ID from SignPath.

The Foundation email confirms eligibility, not production certificate issuance.
Its technical review and certificate provisioning still happen in SignPath.

## GitHub configuration

Open [Settings → Environments](https://github.com/LeenHawk/gproxy/settings/environments),
select the existing **`release`** environment, then add:

| Kind | Name | Value |
| --- | --- | --- |
| Environment variable | `SIGNPATH_ORGANIZATION_ID` | Organization ID from SignPath |
| Environment variable | `SIGNPATH_PROJECT_SLUG` | Project slug |
| Environment variable | `SIGNPATH_SIGNING_POLICY_SLUG` | Production signing policy slug |
| Environment variable | `SIGNPATH_MSIX_PUBLISHER` | Exact full Subject of the production signing certificate, including all DN fields |
| Environment secret | `SIGNPATH_API_TOKEN` | CI submitter API token |
| Environment variable | `SIGNPATH_ENABLED` | `true` when the production certificate and policy are ready; otherwise `false` |

Alternatively, enter the secret interactively with
`gh secret set SIGNPATH_API_TOKEN --env release`. Never paste the token into chat,
command arguments, logs, or committed files.

The existing Release workflow consumes these settings for Windows x86_64 and
ARM64, for regular/Headless CLI ZIPs and regular CLI/Desktop MSIX packages.
No extra workflow is required. MSIX builds retain the existing package names
and filenames; their Publisher uses `SIGNPATH_MSIX_PUBLISHER`. Copy the complete
Subject from the certificate, not the Store Publisher ID or organization name.
With signing enabled, beta, dev and release builds upload the unsigned packages, wait for
SignPath, verify the downloaded signatures and timestamps, regenerate checksums,
then proceed to publication. Update public code-signing documentation after
production activation is confirmed.

## Pipeline contract

`SIGNPATH_ENABLED` unset or `false` preserves unsigned builds during onboarding.
When `true`, all three channels require signing: `main` publishes signed beta
(`staging`) builds, `dev` publishes signed dev (`nightly`) builds, and version
tags publish signed release/prerelease builds according to the existing channel
rules. All use the same production signing policy and certificate.
Disabling the variable later permits unsigned builds again; protect who
can edit repository Actions variables.

The composite action packages each architecture's portable ZIP, uploads it as
`unsigned-gproxy-windows-*` / `unsigned-gproxy-headless-windows-*`, and submits that GitHub artifact. The root ZIP in
the XML is GitHub's artifact envelope. SignPath signs the EXE inside the portable
ZIP. The unsigned Store submission is preserved as an Actions artifact before
CI rebuilds the release MSIX with the signing certificate's Publisher. SignPath
signs that release MSIX and its main executable, then CI verifies both signatures,
timestamps, publisher and package name before replacing the original release
file and regenerating its checksum. Third-party DLLs are not re-signed.
Store submissions keep their original Publisher and go to Partner Center.
The former daily Store synchronization workflow has been removed; WinGet uses
the signed release MSIX directly.

Changing Publisher changes the Windows PackageFamilyName, even when Name and
filename stay the same. A SignPath-signed release is not an in-place update of
an existing Store-identity installation; users must account for that identity
change when switching distribution sources.
These configurations do not use subscription-gated user-defined parameters.
Each wildcard must match exactly one file (SignPath defaults to one match).
Version-specific PE metadata constraints are omitted; the ZIP configuration
retains its fixed product name and original filename constraints.
`wait-for-completion: true` waits for signing and downloads the result; it does
not configure an approval process. The action uses its default completion timeout.

Signed output is downloaded to a separate directory. Windows must validate
the trusted Authenticode signature and timestamp on the portable EXE before
the unsigned archive is replaced. SHA-256 sidecars are then regenerated.
Only these final packages proceed to provenance, the update manifest and
publication. Signing failure has no unsigned fallback when signing is enabled.

This repository change does not provision accounts or certificates. A real
SignPath request and Windows verification remain required;
local syntax checks cannot prove the remote configuration or certificate trust.
