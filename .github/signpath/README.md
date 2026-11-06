# SignPath activation

The project has completed test-certificate onboarding. Production certificate
issuance is still pending. Version releases sign their Windows packages once a
maintainer approves the signing requests in SignPath; see the pipeline contract
below.

## Review material

- Repository: https://github.com/LeenHawk/gproxy
- Downloads: https://gproxy.leenhawk.com/getting-started/downloads/
- Code signing policy: https://gproxy.leenhawk.com/deployment/code-signing/
- Maintainer/PR reviewer: https://github.com/LeenHawk
- License: AGPL-3.0-or-later application, MIT for the crates identified in
  their manifests; no commercial dual-licensing is introduced by this integration.
- Scope: x86_64 and ARM64 portable CLI EXEs and release CLI/Headless/Desktop MSIX packages.
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
  Use `.github/workflows/release.yml` and permit intended version tag (`v*`)
  builds in origin verification.
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
- **Signing Policies**: select the production certificate, grant the CI user
  permission to submit requests and make the maintainers approvers. SignPath
  Foundation requires manual approval of every release signing request.
  Record the signing policy slug.
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
| Environment variable | `SIGNPATH_ENABLED` | `true` once the production certificate and policy are ready; otherwise `false` |

Alternatively, enter the secret interactively with
`gh secret set SIGNPATH_API_TOKEN --env release`. Never paste the token into chat,
command arguments, logs, or committed files.

## Pipeline contract

Only version tags (the `release` channel) are signed; `main` (beta) and `dev`
(nightly) builds stay unsigned. With `SIGNPATH_ENABLED=true` a tagged release:

1. Builds the Windows packages as usual. Each Windows build job also uploads
   its portable ZIP (CLI and Headless) as `signpath-<artifact>.zip` and a
   release MSIX (CLI, Headless and Desktop), packaged with
   `SIGNPATH_MSIX_PUBLISHER` as Publisher, as `signpath-<artifact>.msix`.
   The Store submission MSIX keeps its Partner Center identity and is not signed
   here.
2. The `windows-sign` job submits those ten artifacts to SignPath at once
   (`windows-release` for ZIPs, `windows-msix` for MSIX) and waits up to five
   hours for each request to be approved and signed. Approve them in SignPath.
3. Each signed package is verified on Windows: a trusted, timestamped
   signature on the executable and, for MSIX, on the package, with the
   configured identity and Publisher. The signed file gets its own build
   provenance attestation.
4. `publish` uses the signed packages in place of the unsigned builds, so
   GitHub, GitLab, CNB, `SHA256SUMS`, the update manifests, WinGet and Scoop
   only ever see signed Windows packages.

A request that is denied, fails verification or is not approved within the
wait fails `windows-sign`, and the release is not published. Re-run the failed
jobs (`gh run rerun <run-id> --failed`) to submit new requests; the builds are
not repeated. Set `SIGNPATH_ENABLED=false` to release unsigned packages.

Changing Publisher changes the Windows PackageFamilyName, even when Name and
filename stay the same. A SignPath-signed release is not an in-place update of
an existing Store-identity installation; users must account for that identity
change when switching distribution sources.
These configurations do not use subscription-gated user-defined parameters.
Each wildcard must match exactly one file (SignPath defaults to one match).
Version-specific PE metadata constraints are omitted; the ZIP configuration
retains its fixed product name and original filename constraints.

Allow `.github/workflows/release.yml` on intended `v*` tags in the policy's
origin verification.
