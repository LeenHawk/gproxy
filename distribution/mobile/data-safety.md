# Data safety and privacy worksheet

This is evidence for filling store forms, not a pre-filled declaration. The
publisher does not run a hosted gateway account service, but the application
does transmit information to user-configured endpoints. “No publisher analytics”
does not by itself justify answering “no data collected or shared” in every store.

| Information | Purpose and destination | What to establish for the submission |
| --- | --- | --- |
| Provider accounts, API keys and OAuth tokens | Authentication to the chosen provider; retained in the selected instance database | Actual encryption state on each OS; which provider receives which credentials; deletion and backups |
| Gateway administrator/users and access keys | Local instance administration and client access; selected local or remote database | Distinguish instance accounts from publisher-hosted accounts when answering account-creation/deletion questions |
| Prompts, requests, responses, images, audio and files supplied by clients | Forwarding to the selected AI endpoint and optional configured logging/storage | Applicable data categories, user-initiated transfer disclosures, retention settings and third-party handling |
| Usage, quotas, request errors and timing | Gateway operation and diagnostics in the selected database | Whether content logging is enabled; retention and deletion controls |
| Model-resource downloads, model lists and quota queries | Selected resource/provider endpoints | Network metadata received by each service; whether authentication is sent |
| Interface fonts | Bundled with the application for offline use | No external font requests. License notices are bundled |
| System backups and migration | Android declares `allowBackup=false`; the generated HarmonyOS backup extension is removed | Verify manufacturer-specific device-transfer behavior before making absolute claims about copies outside the sandbox |
| Application updates | F-Droid, Google Play or AppGallery for that distribution | Store policy applies; these builds have no external APK updater |
| User-filed support issues | GitHub, only when the user chooses to post | Issues can be public; leenhawk@leenhawk.com is the private contact channel |

## Answers that cannot be inferred from the repository alone

- The operator's legal name, address, contact email and applicable controller
  information. The current public project name is not a substitute for verified
  business or individual identity required by a store.
- Each upstream provider's retention, processing locations, contractual terms
  and downstream recipients. Users choose their endpoints; the app cannot promise
  that every provider deletes data when local records are removed.
- Whether a specific transfer is exempt from a store's “sharing” declaration
  because it is user-initiated. Apply the current form's definition to the actual
  flow; do not presume an exemption for every API request.
- An unconditional “all data is encrypted in transit” claim. User-configured
  endpoints can use HTTP, and the local gateway's default listener is HTTP on
  loopback. Assess the form's scope and the actual release configurations.
- An unconditional “all stored credentials are encrypted” claim. The application
  reports encryption availability, and mobile storage behavior differs from a
  desktop OS credential store. Do not describe a private app directory as encryption.
- A single fixed retention period. Local logging and retention are configurable;
  remote database, export, backup and provider retention are separate.

## Deletion and review access

Document removal of instance accounts/credentials through supported management
operations, and removal of all private application data via system storage or
uninstall controls. Separately explain exported files, backups, user-selected
remote databases and third-party records. Do not build or advertise a fictitious
publisher account-deletion website for local gateway accounts; answer the account
questions using the actual account model and the store's applicable rules.

Reviewers can use first-run setup without a publisher account. A working model
request needs access to an AI endpoint. Prepare a limited reviewer test endpoint
or clear reproducible instructions if the store requests full-service access.
Do not put personal upstream keys, production customer data or unrestricted API
credentials in public source, screenshots, review videos or fdroiddata metadata.
