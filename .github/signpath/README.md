# SignPath activation

The Foundation application has been submitted and is awaiting review. Do not
enable production signing until the project and certificate are approved.

## Review material

- Repository: https://github.com/LeenHawk/gproxy
- Downloads: https://gproxy.leenhawk.com/getting-started/downloads/
- Code signing policy: https://gproxy.leenhawk.com/deployment/code-signing/
- Maintainer/PR reviewer: https://github.com/LeenHawk
- License: AGPL-3.0-or-later application, MIT for the crates identified in
  their manifests; no commercial dual-licensing is introduced by this integration.
- Scope: x86_64 and ARM64 Windows EXEs, MSI installers, and bundled launchers.
- Build: GitHub-hosted Windows runners, public source and release workflow,
  embedded Console compiled by the same workflow. Windows x86_64 and ARM64
  binaries use UPX before signing.

The policy pages must be deployed before sharing their public URLs with the
reviewer. Confirm GitHub and SignPath MFA, repository permissions, privacy
disclosures, and installer behavior against the Foundation's
[terms](https://signpath.org/terms). Do not mark those checks complete merely
because the policy describes them.

## After approval

1. Install/authorize the SignPath GitHub integration for this repository and
   connect the SignPath project to the trusted GitHub build system. Allow the
   release workflow and intended version tags; do not allow arbitrary branches
   or forks to request production signatures.
2. Add an artifact configuration with slug **`windows-release`**, using
   [windows-release.xml](windows-release.xml). Have SignPath validate this
   configuration against a real uploaded Windows release artifact, including
   MSI extraction paths, metadata and the embedded PowerShell/VBScript files.
3. Configure the production certificate and a signing policy for automatic
   signing, with its approval process disabled. Give the CI submitter the
   required signing-request rights. Review takes place on GitHub pull requests;
   protect `main` with required PR reviews and checks, and cut release tags from
   reviewed commits. The policy and issued certificate must support automatic
   signing; repository configuration cannot override service-side restrictions.
4. Add these repository Actions variables and secret:

   | Kind | Name | Value |
   | --- | --- | --- |
   | Variable | `SIGNPATH_ORGANIZATION_ID` | Approved organization ID |
   | Variable | `SIGNPATH_PROJECT_SLUG` | Approved project slug |
   | Variable | `SIGNPATH_SIGNING_POLICY_SLUG` | Production signing policy slug |
   | Secret | `SIGNPATH_API_TOKEN` | Restricted CI submitter token |
   | Variable | `SIGNPATH_ENABLED` | `true`, only after the above are ready |

   Add the token directly in GitHub's Actions secrets interface or with
   interactive `gh secret set SIGNPATH_API_TOKEN`. Never paste it into chat,
   command arguments, logs, or committed files.
5. Update the pending-review text in README and both languages' download and
   policy pages when approval and activation actually happen. Confirm the team
   roster remains correct.
6. On the first tagged release from reviewed code, confirm that both architecture
   jobs automatically sign and download their packages, then validate the MSI,
   extracted EXE/scripts and portable EXE before publishing.
   Do not use a test certificate for public production releases.

## Pipeline contract

`SIGNPATH_ENABLED` unset or `false` preserves unsigned builds during onboarding.
When `true`, all tag builds (stable and prerelease) require signing. Continuous
`main`/`staging` builds remain unsigned; signing is limited to tagged releases.
Disabling the variable later permits unsigned tag builds again; protect who
can edit repository Actions variables.

The composite action packages each architecture's ZIP and MSI, uploads only
those two files as `unsigned-gproxy-windows-*`, and submits that GitHub artifact.
The root ZIP in the XML is GitHub's artifact envelope. SignPath signs the EXE
inside the portable ZIP and deep-signs the MSI contents before signing the MSI.
The `artifact` and `version` parameters come from release metadata.
`wait-for-completion: true` waits for signing and downloads the result; it does
not configure an approval process. The action uses its default completion timeout.

Signed output is downloaded to a separate directory. Windows must validate
trusted Authenticode signatures and timestamps on all five signed files before
the unsigned packages are replaced. SHA-256 sidecars are then regenerated.
Only these final packages proceed to provenance, the update manifest and
publication. Signing failure has no unsigned fallback when signing is enabled.

No certificate or account was provisioned by this repository change. A real
SignPath request and Windows verification remain required after approval;
local syntax checks cannot prove the remote configuration or certificate trust.
