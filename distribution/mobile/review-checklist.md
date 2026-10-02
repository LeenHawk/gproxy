# First-submission checklist

Scope: Google Play, F-Droid, Huawei Android AppGallery and native HarmonyOS
AppGallery. Huawei distribution is planned for mainland China and overseas.
Both commercial-store developer accounts exist, but their products have not yet
been created. Unchecked items below are not completed by a repository merge.

## Product and operator information

- [ ] Create the Google Play application, confirming `dev.gproxy.desktop` before
  the first upload. Select app signing and upload-key arrangements deliberately.
- [ ] Create the Huawei Android and native HarmonyOS applications in AGC with the
  intended bundle identity; obtain the product IDs and signing materials.
- [ ] Confirm the developer's legal/public display name and usable support/privacy
  email address. Current text uses the project's existing public publisher name
  **Leen Hawk** and GitHub issue contact; it is not a verified mainland operator
  identity, address or private contact channel.
- [ ] In AGC, check the mainland China APP filing and category-specific
  qualification requirements for the actual operator and service model,
  including whether generative-AI-related categories or qualifications apply. Supply
  the approved filing/qualification details where required. Do not insert a
  made-up filing number or assume a provider's registration covers this app.
- [ ] Choose the actual overseas countries/regions and supported devices. Check
  their current console requirements separately; selecting “overseas” in this
  plan does not submit a worldwide release.

## Listing and privacy

- [ ] Review `listings.json`, choosing the Android or HarmonyOS paragraph.
  Provider subscriptions, accounts and charges are not included in GPROXY.
- [ ] Review both bundled privacy notices against the final build and operator
  information, update the notice version if materially changed, then run
  `python3 scripts/mobile/sync-listings.py`.
- [ ] Publish and open both privacy URLs from an unauthenticated browser. Check
  that their contents match the bundled notices. Source files alone are not live
  privacy-policy pages.
- [ ] Complete Google Play Data safety and Huawei privacy questionnaires using
  `data-safety.md` and the actual release behavior, including third-party traffic.
- [ ] Complete content rating, target audience, ads and app-access questionnaires.
  Do not copy a rating from another product or claim a certification not obtained.
- [ ] Supply real screenshots from the final Android application and separately
  from HarmonyOS. Do not present Android screenshots as native HarmonyOS evidence.
- [ ] Add release-specific changelogs after selecting the first published source
  revision and version code. Keep each F-Droid changelog under 500 characters.

## Android permissions and package validation

- [ ] Upload the Play AAB to an internal test track; confirm Play's generated
  APKs, device coverage, app-signing certificate and 16 KiB compatibility report.
- [ ] Test install/update with the intended signing key, including preservation
  of the configured gateway database. A locally generated test key is not proof
  of production update compatibility.
- [ ] Test the offline notice: decline exits; accept opens setup; re-launch skips
  an accepted version; a changed version requires acknowledgement again; boot
  cannot start a gateway before acknowledgement.
- [ ] Test notifications denied, notification Stop/Privacy, backgrounding,
  user-enabled boot start, battery restrictions and device reboot.
- [ ] Submit the Google Play foreground-service declaration and a video showing
  the real user-triggered gateway workflow, visible notification and Stop action.
  Explain `specialUse` as a local gateway service; do not claim the category is
  automatically approved. Store manifests omit `dataSync`; pre-Android-14 store
  builds use an untyped foreground service. Test long-running behavior on each
  supported OS rather than assuming the manifest establishes eligibility.
- [ ] Verify no `REQUEST_INSTALL_PACKAGES`, update Activity/provider or native
  APK download entry point survives in the actual store package.
- [ ] Test on a 16 KiB page-size device/emulator and on a supported older Android
  version. Static ELF/ZIP alignment checks are necessary but not the whole test.
- [ ] If the Play personal account was created after 13 November 2023, complete
  the required closed test (currently 12 opted-in testers continuously for 14
  days), then apply for production access. Confirm the account's current console
  requirement; an internal test is not a substitute for that closed test.

## Native HarmonyOS

- [ ] Run the new store build with the pinned toolchain and pass ArkTS/Hvigor
  compilation, APP/HAP inspections and release signing.
- [ ] Use the app's real release certificate and distribution profile. The
  ordinary unsigned HAP release artifact is not a store-ready signed APP.
- [ ] Test the offline ArkUI notice before Tauri initialization, fresh setup,
  requests through the local gateway, persistence, exit and signed updates.
- [ ] Test each claimed device type. Phone/tablet background behavior must match
  the listing; do not infer it from successful PC or 2-in-1 testing.
- [ ] Request restricted background permission only if needed and eligible. An
  environment variable that adds a permission declaration does not constitute
  Huawei approval or a runtime grant.

## F-Droid

- [ ] Commit and publish the new store-build source. Generate the recipe with
  `scripts/mobile/fdroid-metadata.py` using its full immutable commit.
- [ ] Run fdroiddata metadata lint, update checks, source/APK scanning and the
  isolated build. Review native and JavaScript dependency licenses, not just the
  top-level AGPL file. Do not add broad `scanignore` entries to silence findings.
- [ ] Confirm `NonFreeNet` disclosure and Fastlane descriptions, artwork,
  screenshots and changelogs. F-Droid maintainers decide the final labels.
- [ ] Create the fdroiddata merge request only after the source and recipe are
  available. Its acceptance/publication remains an external review process.
