//! What the operator configures, and the one error type the whole module
//! answers with.
//!
//! # Where the trust root comes from, and why it is not configurable
//!
//! The ed25519 public key is baked in at build time from `GPROXY_UPDATE_PUBKEY`.
//! Signature verification defaults to enabled. Operators can explicitly disable
//! it; artifact size and SHA-256 checks still apply. Without a key, updates are
//! refused while signature verification is enabled.
//!
//! Everything else here *is* configurable, and every value has a flag and a
//! `GPROXY_…` name in [`crate::cli`].

use crate::{
    Error,
    config::{source, text},
};

/// The ed25519 public key manifests are verified against, base64, 32 bytes.
///
/// `option_env!`, so a build without it compiles and then refuses to verify
/// anything. `.github/workflows/release.yml` sets it from
/// `UPDATE_SIGNING_PUBLIC_KEY_B64` and asserts that it decodes to 32 bytes
/// before it builds a single target.
pub const SIGNING_PUBLIC_KEY: Option<&str> = option_env!("GPROXY_UPDATE_PUBKEY");

/// The version this process reports and compares against a manifest.
///
/// The release build sets `GPROXY_BUILD_VERSION`; a source checkout falls back
/// to the crate version, which is what makes `gproxy update --check` work from
/// `cargo run`.
pub const BUILD_VERSION: &str = match option_env!("GPROXY_BUILD_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

/// The channel this binary came from, when the build said so.
pub const BUILD_CHANNEL: &str = match option_env!("GPROXY_BUILD_CHANNEL") {
    Some(channel) => channel,
    None => Channel::RELEASE,
};

/// The commit this binary was built from. The `dev` channel compares this
/// rather than a version, because a rolling build has no version to order.
pub const BUILD_HASH: &str = match option_env!("GPROXY_BUILD_HASH") {
    Some(hash) => hash,
    None => "unknown",
};

/// Which stream of releases to read.
///
/// Three, and the names are the owner's: `dev`, `beta`, `release`. They are
/// **not** v3's — v3 shipped `staging`, `dev` and `releases`, where `dev` meant
/// what `beta` means here. A v3 deployment carrying
/// `GPROXY_UPDATE_CHANNEL=dev` into v4 therefore changes its meaning from
/// "pre-release tags" to "rolling build", which is the one migration note this
/// vocabulary has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Channel {
    /// A rolling build of the default branch. Compared by **commit**, not by
    /// version: there is no ordering to a branch, so "different" is the only
    /// available meaning of "newer".
    Dev,
    /// Main-branch previews and published releases, read through `staging`
    /// and ordered by semver.
    Beta,
    /// Tagged releases. The default, and what a deployment should be on.
    Release,
}

impl Channel {
    pub const RELEASE: &'static str = "release";

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Dev => "dev",
            Self::Beta => "beta",
            Self::Release => Self::RELEASE,
        }
    }

    /// Parse one of the names an operator might type.
    ///
    /// `releases` and `stable` are accepted for `release`: the first is what v3
    /// called it and the second is what people write. `staging` is **not**
    /// accepted — it was v3's name for what is now `dev`, and silently
    /// remapping it would move a deployment between two channels that mean
    /// different things.
    ///
    /// The failure carries no flag name, because there are two callers who
    /// would name different ones: [`UpdateOptions::from_cli`] wraps this in a
    /// [`crate::Error::config`] that names `--update-channel`, while a caller
    /// arriving over HTTP with `?channel=` gets the message as it is.
    pub fn parse(value: &str) -> Result<Self, UpdateError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "dev" | "development" => Ok(Self::Dev),
            "beta" => Ok(Self::Beta),
            "release" | "releases" | "stable" => Ok(Self::Release),
            other => Err(UpdateError::Configuration(format!(
                "`{other}` is not an update channel; expected `dev`, `beta` or `release`"
            ))),
        }
    }

    /// Where the signed manifest for this channel lives when the operator did
    /// not name a URL.
    ///
    /// These three are the client half of the contract
    /// `.github/workflows/release.yml` writes, and each one is a *floating*
    /// location rather than a version, because an updater that had to know the
    /// next version's tag could never find it:
    ///
    /// | Channel | Published by | Resolves through |
    /// |---|---|---|
    /// | `dev` | a push to `dev` | the `nightly` release, force-moved to the new commit |
    /// | `beta` | a push to `main`, or a version release | the `staging` release |
    /// | `release` | a clean version tag | GitHub's own `latest`, which the release is marked as |
    ///
    /// `release` is v3's URL unchanged. The other two are not: v3 published
    /// its rolling channel under a tag literally named `dev`, and v4 uses
    /// `nightly` for it — the tag `dev` is now a *branch*.
    pub fn default_manifest_url(self) -> String {
        Source::Github.manifest_url(self)
    }
}

impl Default for Channel {
    fn default() -> Self {
        // The build's own channel, so a `beta` binary keeps reading `beta`
        // without being told to. A build that named something unparseable
        // reads `release` rather than failing to start.
        Self::parse(BUILD_CHANNEL).unwrap_or(Self::Release)
    }
}

/// Where a channel's signed releases are hosted. The signing identity is the
/// same for all sources; choosing a host never changes signature verification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Github,
    Gitlab,
    Cnb,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Github => "github",
            Self::Gitlab => "gitlab",
            Self::Cnb => "cnb",
        }
    }

    pub fn parse(value: &str) -> Result<Self, UpdateError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "github" => Ok(Self::Github),
            "gitlab" => Ok(Self::Gitlab),
            "cnb" => Ok(Self::Cnb),
            other => Err(UpdateError::Configuration(format!(
                "`{other}` is not an update source; expected `github`, `gitlab` or `cnb`"
            ))),
        }
    }

    pub fn manifest_url(self, channel: Channel) -> String {
        match (self, channel) {
            (Self::Github, Channel::Release) => {
                "https://github.com/LeenHawk/gproxy/releases/latest/download/manifest.json".into()
            }
            (Self::Github, channel) => format!(
                "https://github.com/LeenHawk/gproxy/releases/download/{}/manifest.json",
                if channel == Channel::Dev {
                    "nightly"
                } else {
                    "staging"
                },
            ),
            (Self::Gitlab, channel) => format!(
                "https://gitlab.com/leenhawk1/gproxy/-/releases/{}/downloads/manifest.json",
                match channel {
                    Channel::Dev => "nightly",
                    Channel::Beta => "staging",
                    Channel::Release => "release",
                },
            ),
            (Self::Cnb, channel) => format!(
                "https://cnb.cool/LeenHawk/gproxy/-/releases/download/{}/manifest.json",
                match channel {
                    Channel::Dev => "nightly",
                    Channel::Beta => "staging",
                    Channel::Release => "release",
                },
            ),
        }
    }
}

impl Default for Source {
    fn default() -> Self {
        Self::parse(option_env!("GPROXY_BUILD_UPDATE_SOURCE").unwrap_or("github"))
            .unwrap_or(Self::Github)
    }
}

/// What happens to the process once its executable has been replaced.
///
/// Only the serving path does any of this. `gproxy update` on the command line
/// swaps the file and exits, because there is nothing to restart.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Restart {
    /// Nothing. The operator restarts the process themselves; the new binary
    /// is on disk and takes effect at the next start.
    None,
    /// Exit with code 42 and let the supervisor bring it back. What a systemd
    /// unit with `Restart=always` or a container runtime wants.
    Supervisor,
    /// Replace this process image with the new executable, keeping the same
    /// arguments. The default: it is the only option that needs nothing
    /// outside the process to be configured correctly.
    #[default]
    ReExec,
}

impl Restart {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Supervisor => "supervisor",
            Self::ReExec => "re-exec",
        }
    }

    pub fn parse(value: &str) -> Result<Self, UpdateError> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Ok(Self::None),
            "supervisor" => Ok(Self::Supervisor),
            "re-exec" | "reexec" => Ok(Self::ReExec),
            other => Err(UpdateError::Configuration(format!(
                "`{other}` is not a restart mode; expected `none`, `supervisor` or `re-exec`"
            ))),
        }
    }
}

/// Everything the updater was configured with, layered by
/// [`crate::config::update`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateOptions {
    pub channel: Channel,
    pub source: Source,
    /// The signed manifest to read. `None` means
    /// the selected [`Source::manifest_url`]. An explicit URL takes precedence
    /// over both source and channel defaults.
    pub manifest_url: Option<String>,
    pub restart: Restart,
    /// Seconds between scheduled checks while `serve` runs. `None` switches
    /// the schedule off entirely — no timer, no egress.
    pub interval_secs: Option<u64>,
    /// Install what a scheduled check finds, without being asked.
    ///
    /// **Off unless an operator turned it on.** What it trades away is
    /// written out in `--help`, in the README and in the startup warning: an
    /// instance holding a pile of upstream credentials replacing its own
    /// executable at an hour nobody chose, on a release nobody read the notes
    /// for, and restarting mid-traffic to do it.
    pub automatic: bool,
    pub verify_signature: bool,
}

impl Default for UpdateOptions {
    fn default() -> Self {
        Self {
            channel: Channel::default(),
            source: Source::default(),
            manifest_url: None,
            restart: Restart::default(),
            interval_secs: Some(DEFAULT_INTERVAL_SECS),
            automatic: false,
            verify_signature: true,
        }
    }
}

/// Six hours. Often enough that an operator hears about a release the day it
/// lands, rare enough that ten thousand instances are not a load on the
/// manifest host.
pub const DEFAULT_INTERVAL_SECS: u64 = 6 * 60 * 60;

impl UpdateOptions {
    /// The manifest URL in force for `channel`.
    pub fn manifest_url(&self, channel: Channel, source: Source) -> String {
        self.manifest_url
            .clone()
            .unwrap_or_else(|| source.manifest_url(channel))
    }

    /// Layer the parsed command line into update options.
    ///
    /// Lives here rather than in [`crate::config`] because it is this
    /// module's own configuration and nothing else in the crate reads it — and
    /// because a failure has to name the flag *and* the environment variable,
    /// which is what [`crate::config::source`] is for.
    ///
    /// It is deliberately **not** a field of [`crate::Settings`]. `Settings`
    /// is what every host builds — including the desktop shell, which
    /// constructs one by hand and has no executable of its own to replace
    /// (its updates come from the platform's store). A field there would force
    /// an answer on a host that has none.
    pub fn from_cli(options: &crate::cli::Options) -> Result<Self, Error> {
        let mut resolved = Self::default();
        if let Some(value) = text(&options.update_channel) {
            resolved.channel = Channel::parse(&value)
                .map_err(|error| Error::config(source(crate::cli::UPDATE_CHANNEL), error))?;
        }
        if let Some(value) = text(&options.update_source) {
            resolved.source = Source::parse(&value)
                .map_err(|error| Error::config(source(crate::cli::UPDATE_SOURCE), error))?;
        }
        if let Some(value) = text(&options.update_verify_signature) {
            resolved.verify_signature =
                crate::config::boolean(&value, crate::cli::UPDATE_VERIFY_SIGNATURE)?;
        }
        resolved.manifest_url = text(&options.update_manifest_url);
        if let Some(value) = text(&options.update_restart) {
            resolved.restart = Restart::parse(&value)
                .map_err(|error| Error::config(source(crate::cli::UPDATE_RESTART), error))?;
        }
        if let Some(value) = text(&options.update_check_interval) {
            resolved.interval_secs = interval(&value)?;
        }
        if let Some(value) = text(&options.update_automatic) {
            resolved.automatic = crate::config::boolean(&value, crate::cli::UPDATE_AUTOMATIC)?;
        }
        Ok(resolved)
    }
}

/// Seconds between checks. `0`, `off` and `never` all mean "do not check",
/// because all three are what an operator writes when they mean it.
///
/// A floor of one minute, and it is not tuning: an interval of `1` in a
/// deployment of a thousand instances is a thousand requests a second at the
/// manifest host, and the value that produces it is far more likely to be a
/// unit mistake — seconds where minutes were meant — than a decision.
fn interval(value: &str) -> Result<Option<u64>, Error> {
    const FLOOR: u64 = 60;
    match value.trim().to_ascii_lowercase().as_str() {
        "0" | "off" | "never" | "none" => Ok(None),
        other => match other.parse::<u64>() {
            Ok(seconds) if seconds >= FLOOR => Ok(Some(seconds)),
            Ok(seconds) => Err(Error::config(
                source(crate::cli::UPDATE_CHECK_INTERVAL),
                format!(
                    "{seconds}s is below the {FLOOR}s floor; use `0` to switch checking off \
                     instead"
                ),
            )),
            Err(_) => Err(Error::config(
                source(crate::cli::UPDATE_CHECK_INTERVAL),
                format!("`{other}` is not a number of seconds"),
            )),
        },
    }
}

/// Everything that can go wrong, in v3's taxonomy, because the taxonomy was
/// right: each variant is a different thing for an operator to do about it.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("no update signing key was compiled into this binary, so no manifest can be verified")]
    NoSigningKey,
    #[error("{0}")]
    Configuration(String),
    #[error("signed update manifest is unavailable or invalid")]
    Manifest,
    #[error("update manifest signature verification failed")]
    Signature,
    #[error("the manifest is for the `{found}` channel, not `{expected}`")]
    WrongChannel {
        expected: &'static str,
        found: String,
    },
    #[error("this release has no artifact for `{0}`")]
    Artifact(String),
    #[error("update download failed: {0}")]
    Download(String),
    #[error("downloaded update failed its integrity check")]
    Integrity,
    #[error("verified update archive is invalid")]
    Archive,
    #[error(
        "this release needs data version {required}; this instance is on {current}. Migrate \
         first, or update to a release that does not require it"
    )]
    Incompatible { required: u32, current: u32 },
    #[error("`{0}` is not a version this build can compare")]
    Version(String),
    #[error("executable swap failed")]
    Swap,
    #[error("no rollback executable is available")]
    Rollback,
    #[error("{context}: {error}")]
    Io {
        context: String,
        #[source]
        error: std::io::Error,
    },
}

impl UpdateError {
    pub fn io(context: impl Into<String>, error: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            error,
        }
    }
}

impl From<UpdateError> for gproxy_host_axum::UpdateFailure {
    fn from(error: UpdateError) -> Self {
        use gproxy_host_axum::UpdateFailure as Failure;
        let message = error.to_string();
        match error {
            UpdateError::NoSigningKey | UpdateError::Configuration(_) => {
                Failure::configuration(message)
            }
            UpdateError::Incompatible { .. }
            | UpdateError::Version(_)
            | UpdateError::Rollback
            | UpdateError::Artifact(_)
            | UpdateError::WrongChannel { .. } => Failure::refused(message),
            _ => Failure::upstream(message),
        }
    }
}

impl From<UpdateError> for Error {
    fn from(error: UpdateError) -> Self {
        Self::other(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_channel_names_an_operator_types_all_parse() {
        for (value, expected) in [
            ("release", Channel::Release),
            ("releases", Channel::Release),
            ("STABLE", Channel::Release),
            ("beta", Channel::Beta),
            ("dev", Channel::Dev),
            ("development", Channel::Dev),
        ] {
            assert_eq!(Channel::parse(value).unwrap(), expected, "{value}");
        }
        let error = Channel::parse("nightly").unwrap_err();
        assert!(
            error.to_string().contains("not an update channel"),
            "{error}"
        );
    }

    /// The same bad value, reached through the command line, names the flag
    /// and the variable that would fix it — which is the crate's convention
    /// for every configuration failure.
    #[test]
    fn a_bad_channel_on_the_command_line_names_the_flag_and_the_variable() {
        let error = UpdateOptions::from_cli(&crate::cli::Options {
            update_channel: Some("nightly".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("--update-channel / GPROXY_UPDATE_CHANNEL"),
            "{error}"
        );
    }

    #[test]
    fn restart_defaults_to_reexec_but_allows_an_explicit_override() {
        assert_eq!(Restart::default(), Restart::ReExec);
        assert_eq!(Restart::parse("none").unwrap(), Restart::None);
        assert_eq!(Restart::parse("supervisor").unwrap(), Restart::Supervisor);
        assert_eq!(Restart::parse("reexec").unwrap(), Restart::ReExec);
        assert!(Restart::parse("kill -9").is_err());
    }

    /// The rule the owner set, asserted rather than documented: a default
    /// configuration never installs anything on its own.
    #[test]
    fn automatic_installation_is_off_by_default() {
        assert!(!UpdateOptions::default().automatic);
        assert!(UpdateOptions::default().verify_signature);
        assert!(
            !UpdateOptions::from_cli(&Default::default())
                .unwrap()
                .automatic
        );
        assert_eq!(
            UpdateOptions::default().interval_secs,
            Some(DEFAULT_INTERVAL_SECS)
        );
    }

    #[test]
    fn nothing_said_is_the_default_and_every_value_is_reachable() {
        assert_eq!(
            UpdateOptions::from_cli(&Default::default()).unwrap(),
            UpdateOptions::default()
        );
        let options = UpdateOptions::from_cli(&crate::cli::Options {
            update_channel: Some("dev".into()),
            update_manifest_url: Some("https://mirror.example/manifest.json".into()),
            update_restart: Some("supervisor".into()),
            update_check_interval: Some("900".into()),
            update_automatic: Some("true".into()),
            update_verify_signature: Some("false".into()),
            ..Default::default()
        })
        .unwrap();
        assert_eq!(
            options,
            UpdateOptions {
                channel: Channel::Dev,
                source: Source::default(),
                manifest_url: Some("https://mirror.example/manifest.json".into()),
                restart: Restart::Supervisor,
                interval_secs: Some(900),
                automatic: true,
                verify_signature: false,
            }
        );
    }

    #[test]
    fn checking_can_be_switched_off_but_not_turned_into_a_flood() {
        for value in ["0", "off", "never", "none"] {
            let options = UpdateOptions::from_cli(&crate::cli::Options {
                update_check_interval: Some(value.into()),
                ..Default::default()
            })
            .unwrap();
            assert_eq!(options.interval_secs, None, "{value}");
        }
        // A value below the floor is refused rather than silently raised: it
        // is much more likely a unit mistake than a decision.
        let error = UpdateOptions::from_cli(&crate::cli::Options {
            update_check_interval: Some("5".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(error.to_string().contains("floor"), "{error}");
        let error = UpdateOptions::from_cli(&crate::cli::Options {
            update_check_interval: Some("six hours".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(error.to_string().contains("not a number"), "{error}");
    }

    #[test]
    fn each_channel_has_its_own_manifest_when_none_was_named() {
        let options = UpdateOptions::default();
        let urls: Vec<String> = [Channel::Release, Channel::Beta, Channel::Dev]
            .into_iter()
            .map(|channel| options.manifest_url(channel, Source::Github))
            .collect();
        assert_eq!(urls.len(), 3);
        assert!(urls[0].contains("/latest/download/"), "{}", urls[0]);
        assert!(urls[1].ends_with("staging/manifest.json"), "{}", urls[1]);
        // `nightly`, not `dev`: the pipeline publishes the rolling channel
        // under a floating release tagged `nightly`, and `dev` is a branch.
        assert!(urls[2].ends_with("nightly/manifest.json"), "{}", urls[2]);

        // A named URL wins for every channel: that is what makes a private
        // mirror, and the end-to-end test, possible.
        let named = UpdateOptions {
            manifest_url: Some("http://127.0.0.1:9/manifest.json".into()),
            ..UpdateOptions::default()
        };
        for source in [Source::Github, Source::Gitlab, Source::Cnb] {
            assert_eq!(
                named.manifest_url(Channel::Dev, source),
                "http://127.0.0.1:9/manifest.json"
            );
        }
    }

    #[test]
    fn sources_resolve_independent_channels_and_cli_rejects_unknown_hosts() {
        for channel in [Channel::Dev, Channel::Beta, Channel::Release] {
            let github = Source::Github.manifest_url(channel);
            let cnb = Source::Cnb.manifest_url(channel);
            assert!(github.starts_with("https://github.com/LeenHawk/gproxy/"));
            assert!(cnb.starts_with("https://cnb.cool/LeenHawk/gproxy/-/releases/download/"));
            assert!(cnb.ends_with(&format!(
                "{}/manifest.json",
                match channel {
                    Channel::Dev => "nightly",
                    Channel::Beta => "staging",
                    Channel::Release => "release",
                }
            )));
        }
        for (channel, url) in [
            (
                Channel::Dev,
                "https://gitlab.com/leenhawk1/gproxy/-/releases/nightly/downloads/manifest.json",
            ),
            (
                Channel::Beta,
                "https://gitlab.com/leenhawk1/gproxy/-/releases/staging/downloads/manifest.json",
            ),
            (
                Channel::Release,
                "https://gitlab.com/leenhawk1/gproxy/-/releases/release/downloads/manifest.json",
            ),
        ] {
            assert_eq!(Source::Gitlab.manifest_url(channel), url);
        }
        for (name, source) in [
            ("github", Source::Github),
            ("gitlab", Source::Gitlab),
            ("cnb", Source::Cnb),
        ] {
            let options = UpdateOptions::from_cli(&crate::cli::Options {
                update_source: Some(name.into()),
                ..Default::default()
            })
            .unwrap();
            assert_eq!(options.source, source);
            assert_eq!(options.source.as_str(), name);
        }
        let error = UpdateOptions::from_cli(&crate::cli::Options {
            update_source: Some("untrusted".into()),
            ..Default::default()
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .starts_with("--update-source / GPROXY_UPDATE_SOURCE")
        );
    }

    #[test]
    fn a_refusal_and_a_failure_are_not_the_same_status() {
        use gproxy_host_axum::update::UpdateFailureKind as Kind;
        let refused: gproxy_host_axum::UpdateFailure = UpdateError::Incompatible {
            required: 5,
            current: 4,
        }
        .into();
        assert_eq!(refused.kind, Kind::Refused);
        let upstream: gproxy_host_axum::UpdateFailure = UpdateError::Integrity.into();
        assert_eq!(upstream.kind, Kind::Upstream);
        let configuration: gproxy_host_axum::UpdateFailure = UpdateError::NoSigningKey.into();
        assert_eq!(configuration.kind, Kind::Configuration);
    }
}
