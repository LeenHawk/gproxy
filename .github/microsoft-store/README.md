# Microsoft Store publication

GPROXY has two independent Microsoft Store products, both published:
[GPROXY Desktop](https://apps.microsoft.com/detail/9P2FJRB9RS4Z) and
[GPROXY CLI](https://apps.microsoft.com/detail/9NBMH3S5K0L9). Renaming the original
product keeps its package identity and update history.

| Edition | Display name | Product ID | Package identity |
| --- | --- | --- | --- |
| Desktop | GPROXY Desktop | 9P2FJRB9RS4Z | LeenHawk.GPROXYGateway |
| CLI | GPROXY CLI | 9NBMH3S5K0L9 | LeenHawk.GPROXYCLI |

The shared publisher is `CN=7D76D0DD-9AFE-4262-832E-3A611C4CB5C3`; its display
name is `Leen Hawk`. The CLI identity is assigned by Partner Center, not derived
by appending `.CLI` to the desktop identity.

The graphical application's Tauri/bundle identifier is `com.leenhawk.gproxy.app`.
The CLI edition already exists as **GPROXY CLI**, with its own Store product and
MSIX identity shown above. `com.leenhawk.gproxy.cli` names that existing edition
in the project naming scheme; it does not mean the CLI is an unimplemented product.
These project identifiers do not replace the Microsoft-assigned MSIX package
identities above. Both existing Store products are retained; no replacement products
are created. Store packages must keep the exact identities from Partner Center.

## Configuration

Configure these repository Actions variables from Partner Center:

| Desktop | CLI |
| --- | --- |
| `MS_STORE_PRODUCT_ID` | `MS_STORE_CLI_PRODUCT_ID` |
| `MS_STORE_IDENTITY_NAME` | `MS_STORE_CLI_IDENTITY_NAME` |
| `MS_STORE_DISPLAY_NAME` | `MS_STORE_CLI_DISPLAY_NAME` |

Both editions use `MS_STORE_IDENTITY_PUBLISHER`,
`MS_STORE_PUBLISHER_DISPLAY_NAME`, `MS_STORE_TENANT_ID`, `MS_STORE_SELLER_ID`,
`MS_STORE_CLIENT_ID`, and the `MS_STORE_CLIENT_SECRET` Actions secret. Set
`MS_STORE_PUBLISH_ENABLED=true` to enable stable-release submissions.
The Entra application must have the Partner Center Manager role.

Creating a product, reserving its name and completing its first age-rating
questionnaire are Partner Center onboarding steps. The submission API does not
create a new MSIX product. Complete each first submission in Partner Center;
the automatic publisher skips products that have not yet been published.

## Stable release updates

Tagged stable releases call `store-publish.yml` after publishing the GitHub
release. Desktop and CLI jobs each download their own x64 and ARM64 artifacts,
validate the exact package identity, publisher, architecture and release
version, and create a bundle. Every version comes from the release workflow's
`version` input; no release version is embedded in these scripts.

The publisher checks the corresponding Partner Center product and refuses to
replace an existing draft or active review. It skips duplicate or older
versions. Microsoft Store CLI uploads a new draft with `--noCommit`, then the
script updates release notes from the selected GitHub release and commits that
draft. The notes are limited to the Store field length, with the full release
URL retained. Other listing text, screenshots and publishing settings are
preserved. Failed metadata updates leave an uncommitted draft for inspection.

Store jobs use the protected `release` environment. Nightly/dev, beta/staging
and prerelease builds never submit to Microsoft Store. Store publication does
not modify any of those update channels.

## Retry or prepare a first submission

Dispatch **Publish Microsoft Store** with an existing stable version (without
`v`) and the corresponding Release workflow run ID. The workflow verifies the
source workflow, tag, commit and public stable release before using its files.

- Default: use the retained MSIX artifacts and submit both edition updates.
- `prepare_only=true`: produce upload bundles without accessing Store
  credentials or submitting. This uses the `store-packaging` environment,
  keeping package preparation separate from protected release publication.
- `repackage=true`: download the selected release's portable ZIPs, verify
  `SHA256SUMS` and executable product versions, and package the same binaries
  using the current Partner Center names and identities. This supports first
  submissions or a reserved display-name change without modifying released
  binaries or replacing public release attachments.

The two `microsoft-store-<edition>-<version>` artifacts contain the bundles for
manual upload. A preparation job succeeding means the packages were built and
validated; it does not mean they were submitted or certified. A publishing job
succeeding means the submission was committed; certification remains a Store
process.

## Packages and listing material

`scripts/package-windows-msix.ps1 -Mode application` uses the desktop identity
variables. `-Mode cli` uses the CLI identity variables. Explicit `-IdentityName`
and `-DisplayName` values are used exactly as supplied. Both modes require the
real shared publisher values. Configure the same public identity variables in
any other CI service that invokes this packaging script.

Store submission artifacts and preparation bundles retain their Partner Center
identity and are unsigned for submission. Microsoft signs the Store-distributed
copies after certification.

GitHub release MSIX packages keep the existing filenames. When SignPath is
enabled, CI rebuilds them with the certificate's Publisher, signs the main EXE
and MSIX, verifies the signatures, and publishes them directly. See
[SignPath configuration](../signpath/README.md). The daily Store package
synchronization workflow has been removed; WinGet consumes signed release assets.

Use [listing.md](listing.md), [listings.json](listings.json) and
[PRIVACY.md](../../PRIVACY.md) for the bilingual/trilingual submission material.
Test first-run setup, launch, listener access, persistence, restart, uninstall
and updates on Windows. Do not claim accessibility certification, performance
numbers or hardware requirements without supporting evidence.
