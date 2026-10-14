# GPROXY privacy information

Last updated: 2026-10-02

This information applies to the open-source GPROXY Desktop, Android, HarmonyOS and GPROXY CLI
applications published by Leen Hawk. Source code and the AGPL-3.0-or-later
license are available at <https://github.com/LeenHawk/gproxy>.

## Information handled by the application

GPROXY runs an AI API gateway on your device or infrastructure. It handles
the provider accounts and API credentials you configure, gateway user accounts
and access keys, model requests and responses, and operational records such as
usage, quotas, request status and timing. Requests and responses can contain
personal information, prompts, text, images, audio, files or other content
supplied by you or clients connected to your gateway.

Configuration, credentials and operational records are stored in the database
and storage locations selected for your instance. What request content is
recorded, and how long it is kept, depends on your logging and retention
settings. A database you configure on another server is not local-only storage.

Desktop uses the operating system credential store when available and reports
when it falls back to file storage. CLI supports encryption of stored upstream
credentials with a configured `GPROXY_MASTER_KEY`; without a master key, those
credentials are stored unencrypted. Keep your database, backups and encryption
keys private. These mechanisms do not prevent the configured upstream provider
from receiving requests.

Android and HarmonyOS builds report whether credential encryption is available.
Do not assume stored credentials are encrypted; an app-private directory is not
encryption.

## Information sent over the network

GPROXY sends requests and the necessary credentials to the AI providers and
other endpoints you configure. Those services process information under their
own terms and privacy policies. GPROXY does not provide model accounts or
include a hosted AI service operated by the project publisher.

Network connections can also be made for the features you use, including
provider sign-in, model and quota queries, remote databases, and downloading
model resources or application updates. The receiving service can see ordinary
connection information such as your IP address. Direct-distribution interface fonts may be fetched
from `https://gproxy.leenhawk.com/fonts/`; font requests do not include gateway
credentials or model request content. Mobile store builds include these fonts
locally. Store-installed application updates are
delivered by the corresponding store, subject to that store's policies.

Mobile store builds show the bundled privacy notice before starting the gateway
or loading the application WebView. The mobile notice is maintained in
[`distribution/mobile/privacy`](distribution/mobile/privacy) and supplies the
English and Chinese website privacy pages. The notice can also be read offline
in the application's settings.

GPROXY does not automatically send gateway information to the project publisher,
including for analytics or support. Information you choose to post in GitHub
issues is handled by GitHub and may be public. Never post credentials, access
tokens, or private request content in an issue; use email for anything private.

## Your choices and deletion

You choose which providers, storage backends and clients to use. The listener
defaults to a local loopback address. Exposing the gateway to a network, adding
users or changing access rules changes who can reach your instance.

Use the application's management interface to remove configured accounts,
credentials or records where supported. To remove all locally stored instance
data, stop the application and delete its data directory and any associated
backups. Remove application credentials from the operating system credential
store if applicable. Uninstalling a portable or CLI application alone may leave
your chosen data directory intact. Remote databases and provider-side records
must be managed separately; local deletion does not erase a provider's copies.

## Contact

For privacy questions, use <https://github.com/LeenHawk/gproxy/issues> without
including personal data or secrets, or email <leenhawk@leenhawk.com> privately. Product documentation is available at
<https://gproxy.leenhawk.com/>. Changes to application behavior may require this
information to be updated alongside a release.
