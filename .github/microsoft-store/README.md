# Microsoft Store onboarding

MSIX replaces the Windows MSI. Store signing and distribution are pending;
GitHub Releases offer the portable ZIP and unsigned Tauri MSIX packages.
SignPath still applies only to the portable EXE.

## Account and product

1. Register at <https://developer.microsoft.com/microsoft-store/register/>.
   Choose the account type matching the actual publisher. Personal account
   login, identity verification and agreement acceptance are done by the owner.
2. In Partner Center's Apps and games workspace, create an **MSIX or PWA app**
   and reserve the display name **GPROXY Gateway**.
3. Open the product's **Product identity** page. Copy these public values
   exactly; do not invent a publisher or use a local test certificate:

   | Partner Center field | Repository Actions variable |
   | --- | --- |
   | Reserved product name | `MS_STORE_DISPLAY_NAME` |
   | Package/Identity/Name | `MS_STORE_IDENTITY_NAME` |
   | Package/Identity/Publisher | `MS_STORE_IDENTITY_PUBLISHER` |
   | Package/Properties/PublisherDisplayName | `MS_STORE_PUBLISHER_DISPLAY_NAME` |

   The Store product ID can be used for the eventual public download link.
   These values are public identifiers, not passwords or signing secrets.
   Do not send account credentials or identity documents to the repository.

## Build and inspect

The Release application's Windows jobs build x64 and ARM64 Tauri MSIX packages
for nightly, prerelease and stable releases. All four Store identity variables
are required. CI also builds the application on both architectures and validates
Store packaging for same-repository builds using a stable workspace version;
this CI-only validation skips an entirely unconfigured identity.

Stable releases retain unsigned MSIX files in Actions artifacts named
`microsoft-store-unsigned-gproxy-tauri-windows-*` for 30 days. The same unsigned
MSIX files are also published on GitHub, with build attestations. They require
signing before normal installation; Microsoft signs the Store-distributed copies
after certification. No separate GitHub MSIX signing service is configured.

To reproduce on Windows, build the `gproxy-desktop.exe` Tauri target first, then
run `scripts/package-windows-msix.ps1` with `-Target`, `-Artifact`, `-Version`,
`-IdentityName`, `-DisplayName`, `-Publisher` and `-PublisherDisplayName`. Use the
four real Partner Center values. The script maps the version to `major.minor.patch.0`,
packages the Tauri executable and project icons, and invokes Windows SDK MakeAppx
with manifest validation enabled. Default output is `dist/store/*.msix`.

Before submission, run the Windows App Certification Kit and install a locally
signed test copy on Windows 10 version 2004 or later and Windows 11. Test both
architectures where hardware is available. Never distribute a local test
certificate as the production installation path.

Verify first-run administrator setup, browser opening, loopback inference,
background process/logs, Windows Startup settings, restart, uninstall and Store
update behavior. Data and the generated master key live under
`%LOCALAPPDATA%\Packages\<PackageFamilyName>\LocalState\GPROXY`.
MSIX uninstall removes package-private data. Export/back up before uninstalling.

Existing MSI installs are not silently migrated. Export configuration, stop
the old process and disable its startup entry before launching Store GPROXY,
then import into the new instance. Back up the original DB and master key.

## Automatic release updates

Stable tag releases call `.github/workflows/store-publish.yml` after the GitHub
Release is published. It uses the official
[`microsoft/microsoft-store-apppublisher@v1.4`](https://github.com/microsoft/microsoft-store-apppublisher)
Action with Microsoft Store CLI v0.4.2. Windows SDK bundles the x64 and ARM64
MSIX files into one `.msixbundle`; the official `msstore publish` command uploads
it and commits the update. Existing listings, screenshots and publishing settings
are inherited from the Store submission.

Configure once, after the first manual Store submission is published:

| Kind | Name | Value |
| --- | --- | --- |
| Actions variable | `MS_STORE_PRODUCT_ID` | `9P2FJRB9RS4Z` (already configured) |
| Actions variable | `MS_STORE_TENANT_ID` | Microsoft Entra tenant ID |
| Actions variable | `MS_STORE_SELLER_ID` | Partner Center seller ID, from Account settings / Legal info |
| Actions variable | `MS_STORE_CLIENT_ID` | Entra application ID with Partner Center Manager role |
| Actions secret | `MS_STORE_CLIENT_SECRET` | Secret for that Entra application |
| Actions variable | `MS_STORE_PUBLISH_ENABLED` | `true` once authorization and the first publication are complete |

Associate the Entra application with Partner Center under Account settings /
Users and give it the Manager role. Follow Microsoft's
[submission API setup](https://learn.microsoft.com/en-us/windows/uwp/monetize/create-and-manage-submissions-using-windows-store-services#how-to-associate-an-azure-ad-application-with-your-partner-center-account).
Add the secret through GitHub Actions secrets or interactive
`gh secret set MS_STORE_CLIENT_SECRET`; do not paste it into chat or source.
The workflow uses it through environment variables on a disposable GitHub-hosted
Windows runner. CLI credential storage is not cached or uploaded. Configure the
CLI before changing its settings; its interactive reset command is not used in CI.

The job validates both package identities and versions before upload. It skips
the update with a notice until the first manual submission is published. It also
skips an already published version or an older release, and refuses to replace a
pending submission because the official CLI otherwise deletes it. This preserves
manual drafts and ongoing reviews. Store submissions run serially without
cancelling an active upload. Prerelease and staging builds do not submit to Store.

A successful job means Microsoft accepted the upload and submission commit;
certification and public availability remain Microsoft's processing steps.
If another submission is pending, finish it in Partner Center and rerun the
failed Store job. The job does not recreate the first submission or rewrite
store text/screenshots on each release.

To retry an existing release with a newer workflow fix, run **Publish Microsoft
Store** manually on `main`, supplying its version (without `v`) and the original
Release workflow run ID. This reuses the MSIX artifacts without rebuilding or
changing a public release tag. The workflow verifies the source run, public
release and tag commit before downloading packages.

## Submission material

Create a submission for the reserved product, upload both architecture MSIX
packages, and use [listing.md](listing.md) as a starting point for listing text
and the `runFullTrust` justification. Supply real product screenshots and
complete Partner Center's age-rating and availability questions accurately.
The privacy description is published at
<https://gproxy.leenhawk.com/deployment/code-signing/#privacy>; deploy the revised
policy before using it in the listing. Confirm the product's support URL and
publisher display name with the account owner.

MSIX contains `runFullTrust` because it launches a native Rust HTTP gateway and
its first-run setup helper. It does not install a system service or request
administrator privileges. Windows package identity disables native update,
rollback, GitHub update notices and registry-based startup management. Updates
come from Store; startup is a manifest StartupTask controlled by Windows.

Microsoft re-signs the package after certification. This does not sign the
portable ZIP's EXE or issue a reusable Microsoft certificate to the project.
Publish a Store download link only after the listing is actually available.
