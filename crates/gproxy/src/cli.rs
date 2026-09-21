//! The argument grammar, and the single source of truth for environment names.
//!
//! # Why every field carries `env`
//!
//! `--help` is the documentation an operator actually reads, and a separate
//! list of environment variables in a README is a list that goes stale. So
//! there is no separate list: every configurable value is one `clap` field with
//! `env = …` on it, the names are the constants below, and `--help` prints the
//! environment variable next to the flag it shadows. A value that cannot be
//! reached both ways does not exist.
//!
//! # Why everything is `Option<String>`
//!
//! `clap` could parse a `u16` for `--port`, but then the error would be
//! `clap`'s and the flag would be the only thing named in it. Parsing happens
//! in [`crate::config`] instead, where the message can name the flag *and* the
//! environment variable, and where a value that came from a config file goes
//! through exactly the same validation. `Option` also carries the one fact the
//! layering needs: whether the operator said anything at all.
//!
//! # Why the names are v3's
//!
//! An upgrade should not be a redeployment. Every name here that existed in v3
//! means the same thing it did there, so an existing unit file, compose file or
//! `.env` keeps working.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

// The environment names, as constants rather than literals: the same symbol is
// what `clap` advertises in `--help` and what an error message quotes back.
pub const CONFIG: &str = "GPROXY_CONFIG";
pub const HOST: &str = "GPROXY_HOST";
pub const PORT: &str = "GPROXY_PORT";
pub const DATA_DIR: &str = "GPROXY_DATA_DIR";
pub const PERSISTENCE: &str = "GPROXY_PERSISTENCE";
pub const DSN: &str = "GPROXY_DSN";
pub const REDIS_URL: &str = "GPROXY_REDIS_URL";
pub const MASTER_KEY: &str = "GPROXY_MASTER_KEY";
pub const MASTER_KEY_NEXT: &str = "GPROXY_MASTER_KEY_NEXT";
pub const MASTER_KEY_ROTATE: &str = "GPROXY_MASTER_KEY_ROTATE";
pub const PUBLIC_BASE_URL: &str = "GPROXY_PUBLIC_BASE_URL";
pub const CORS_ORIGINS: &str = "GPROXY_CORS_ORIGINS";
pub const TRUSTED_PROXIES: &str = "GPROXY_TRUSTED_PROXIES";
pub const FILE_STORAGE_DIR: &str = "GPROXY_FILE_STORAGE_DIR";
pub const CONSOLE: &str = "GPROXY_CONSOLE";
pub const CONSOLE_PATH: &str = "GPROXY_CONSOLE_PATH";
pub const INSTANCE_ID: &str = "GPROXY_INSTANCE_ID";
pub const LOG_FORMAT: &str = "GPROXY_LOG_FORMAT";
pub const LOG_FILTER: &str = "GPROXY_LOG_FILTER";
pub const ADMIN_USER: &str = "GPROXY_ADMIN_USER";
pub const ADMIN_PASSWORD: &str = "GPROXY_ADMIN_PASSWORD";
pub const BOOTSTRAP_ADMIN_API_KEY: &str = "GPROXY_BOOTSTRAP_ADMIN_API_KEY";
pub const IMPORT_SOURCE_MASTER_KEY: &str = "GPROXY_IMPORT_SOURCE_MASTER_KEY";

/// The `.env` file, read before `clap` parses. Only the real environment can
/// name it, because a flag would have to be parsed by the very step it feeds.
pub const ENV_FILE: &str = "GPROXY_ENV_FILE";

#[derive(Debug, Default, Parser)]
#[command(
    name = "gproxy",
    version,
    about = "GPROXY — one gateway in front of many LLM providers",
    long_about = "GPROXY — one gateway in front of many LLM providers.\n\n\
                  Configuration is layered: a flag beats an environment variable, \
                  an environment variable beats `.env`, and `.env` beats the file \
                  named by --config. Every flag below shows the environment \
                  variable that sets the same value.",
    // `serve` is what running the binary with no arguments should do, and it
    // is the only command a deployment ever invokes.
    args_conflicts_with_subcommands = false
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub options: Options,
}

/// Everything that configures the instance.
///
/// Flattened into the root and marked `global`, so `gproxy --port 9000 serve`
/// and `gproxy serve --port 9000` are the same invocation. An operator should
/// not have to remember which side of the subcommand a flag belongs on.
#[derive(Debug, Default, Args)]
pub struct Options {
    /// TOML file holding any AppConfig field. The weakest source: anything on
    /// the command line, in the environment or in `.env` overrides it.
    #[arg(long, short = 'c', global = true, env = CONFIG, value_name = "PATH")]
    pub config: Option<PathBuf>,

    /// Address to listen on. [default: 127.0.0.1]
    #[arg(long, global = true, env = HOST, value_name = "ADDRESS")]
    pub host: Option<String>,

    /// Port to listen on. [default: 7070]
    #[arg(long, short = 'p', global = true, env = PORT, value_name = "PORT")]
    pub port: Option<String>,

    /// Directory relative paths resolve against: the SQLite file, local file
    /// storage, downloaded vocabularies. [default: data]
    #[arg(long, global = true, env = DATA_DIR, value_name = "PATH")]
    pub data_dir: Option<String>,

    /// Database backend: `sqlite`, `postgres` or `mysql`. [default: sqlite]
    #[arg(long, global = true, env = PERSISTENCE, value_name = "BACKEND")]
    pub persistence: Option<String>,

    /// Connection string. A path (or `sqlite://` URL) for SQLite, a
    /// `postgres://` or `mysql://` URL otherwise. Naming one without
    /// --persistence infers the backend from its scheme.
    #[arg(long, global = true, env = DSN, value_name = "DSN", hide_env_values = true)]
    pub dsn: Option<String>,

    /// Redis to share TTL state, rate-limit counters and invalidation notices
    /// through. Required when more than one instance serves one database.
    #[arg(long, global = true, env = REDIS_URL, value_name = "URL", hide_env_values = true)]
    pub redis_url: Option<String>,

    /// The 32-byte key credential secrets are sealed with, as 64 hex
    /// characters or base64. Unset stores secrets in plaintext.
    #[arg(long, global = true, env = MASTER_KEY, value_name = "KEY", hide_env_values = true)]
    pub master_key: Option<String>,

    /// The key to re-seal to. Does nothing on its own; see
    /// --master-key-rotate.
    #[arg(long, global = true, env = MASTER_KEY_NEXT, value_name = "KEY", hide_env_values = true)]
    pub master_key_next: Option<String>,

    /// Re-seal every stored secret from --master-key to --master-key-next at
    /// startup. Takes an optional boolean so a stray empty next key cannot
    /// rotate anything by accident.
    #[arg(
        long,
        global = true,
        env = MASTER_KEY_ROTATE,
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true"
    )]
    pub master_key_rotate: Option<String>,

    /// The instance's external origin, e.g. https://gproxy.example.com. Used
    /// to mint publication links and the OAuth issuer identifier.
    #[arg(long, global = true, env = PUBLIC_BASE_URL, value_name = "URL")]
    pub public_base_url: Option<String>,

    /// Browser origin allowed to call the APIs; comma-separated in the
    /// environment. Empty means same-origin only.
    #[arg(long = "cors-origin", global = true, env = CORS_ORIGINS, value_name = "ORIGIN")]
    pub cors_origins: Option<String>,

    /// Peer whose x-forwarded-for and x-forwarded-proto are believed;
    /// comma-separated in the environment. Empty trusts nothing.
    #[arg(long = "trusted-proxy", global = true, env = TRUSTED_PROXIES, value_name = "ADDRESS")]
    pub trusted_proxies: Option<String>,

    /// Local directory for published bodies and downloaded vocabularies.
    /// Unset leaves both disabled.
    #[arg(long, global = true, env = FILE_STORAGE_DIR, value_name = "PATH")]
    pub file_storage_dir: Option<String>,

    /// Serve the console. [default: true]
    #[arg(
        long,
        global = true,
        env = CONSOLE,
        value_name = "BOOL",
        num_args = 0..=1,
        default_missing_value = "true"
    )]
    pub console: Option<String>,

    /// Serve the console from this directory instead of the bundle compiled
    /// into the binary, for development against a Vite build.
    #[arg(long, global = true, env = CONSOLE_PATH, value_name = "PATH")]
    pub console_path: Option<String>,

    /// A stable name for this process, recorded with continuations that hold a
    /// live upstream connection. Random when unset.
    #[arg(long, global = true, env = INSTANCE_ID, value_name = "ID")]
    pub instance_id: Option<String>,

    /// Log format: `text` or `json`. [default: text]
    #[arg(long, global = true, env = LOG_FORMAT, value_name = "FORMAT")]
    pub log_format: Option<String>,

    /// Tracing filter, in `RUST_LOG` syntax. Falls back to RUST_LOG, then to
    /// `info`.
    #[arg(long, global = true, env = LOG_FILTER, value_name = "FILTER")]
    pub log_filter: Option<String>,

    /// Name of the administrator created on a first start. [default: admin]
    #[arg(long = "admin-user", alias = "user", global = true, env = ADMIN_USER, value_name = "NAME")]
    pub admin_user: Option<String>,

    /// Password for that administrator. One is generated and printed once when
    /// this is unset.
    #[arg(
        long = "admin-password",
        alias = "password",
        global = true,
        env = ADMIN_PASSWORD,
        value_name = "PASSWORD",
        hide_env_values = true
    )]
    pub admin_password: Option<String>,

    /// Mint this exact gateway API key for the administrator on a first start,
    /// instead of a random one.
    #[arg(
        long = "admin-api-key",
        alias = "api-key",
        global = true,
        env = BOOTSTRAP_ADMIN_API_KEY,
        value_name = "KEY",
        hide_env_values = true
    )]
    pub admin_api_key: Option<String>,
}

#[derive(Debug, Clone, Subcommand)]
pub enum Command {
    /// Serve the gateway. This is what running `gproxy` with no subcommand
    /// does.
    Serve,

    /// Create or incrementally synchronize the database schema, then exit.
    ///
    /// `serve` does this too; the separate command exists for a deployment
    /// that runs migrations as their own step, with one writer, before any
    /// instance starts.
    Migrate,

    /// Create the first administrator, idempotently.
    Bootstrap {
        #[command(subcommand)]
        target: BootstrapTarget,
    },

    /// Write this instance's configuration to a JSON document.
    Export {
        /// Where to write. `-` writes to standard output.
        #[arg(long, short = 'o', value_name = "PATH")]
        out: PathBuf,

        /// Include the sealed credential secrets. The document is then exactly
        /// as sensitive as the database it came from.
        #[arg(long)]
        include_secrets: bool,
    },

    /// Replay a configuration document into this instance.
    Import {
        /// The document to read. `-` reads standard input. Mutually exclusive
        /// with --from-v3.
        #[arg(
            long = "in",
            short = 'i',
            value_name = "PATH",
            required_unless_present = "from_v3"
        )]
        input: Option<PathBuf>,

        /// Migrate a **v3** deployment instead: the document is v3's own
        /// export (`POST /admin/api/export`), not this command's. Providers,
        /// credentials, routing, pricing, quotas, users and API keys come
        /// across; usage and logs do not. Requires a database that is empty
        /// apart from an earlier run of the same import.
        #[arg(long, value_name = "PATH", conflicts_with = "input")]
        from_v3: Option<PathBuf>,

        /// `merge` writes what the document names and leaves the rest alone;
        /// `replace` additionally deletes rows of an exported kind that the
        /// document does not mention. Not used by --from-v3, which only ever
        /// merges into a database it has checked is otherwise empty.
        #[arg(long, value_enum, default_value_t = ImportModeArg::Merge)]
        mode: ImportModeArg,

        /// The master key the source instance sealed its secrets with, as hex
        /// or base64. With it every secret is opened and re-sealed under this
        /// instance's key; without it the blobs are stored as they arrived.
        /// For --from-v3 this is the v3 instance's `GPROXY_MASTER_KEY`, and a
        /// sealed v3 export cannot be imported without it.
        #[arg(
            long,
            env = IMPORT_SOURCE_MASTER_KEY,
            value_name = "KEY",
            hide_env_values = true
        )]
        source_master_key: Option<String>,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum BootstrapTarget {
    /// The instance administrator. Does nothing when any user already exists.
    Admin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ImportModeArg {
    Merge,
    Replace,
}

impl From<ImportModeArg> for gproxy_sdk::dto::ImportMode {
    fn from(mode: ImportModeArg) -> Self {
        match mode {
            ImportModeArg::Merge => Self::Merge,
            ImportModeArg::Replace => Self::Replace,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_grammar_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_subcommand_is_serve() {
        let cli = Cli::try_parse_from(["gproxy"]).unwrap();
        assert!(cli.command.is_none());
    }

    #[test]
    fn a_global_flag_is_accepted_on_either_side_of_the_subcommand() {
        for argv in [
            vec!["gproxy", "--port", "9000", "serve"],
            vec!["gproxy", "serve", "--port", "9000"],
        ] {
            let cli = Cli::try_parse_from(&argv).unwrap();
            assert_eq!(cli.options.port.as_deref(), Some("9000"), "{argv:?}");
            assert!(matches!(cli.command, Some(Command::Serve)));
        }
    }

    #[test]
    fn rotation_is_armed_with_or_without_a_value() {
        let bare = Cli::try_parse_from(["gproxy", "--master-key-rotate"]).unwrap();
        assert_eq!(bare.options.master_key_rotate.as_deref(), Some("true"));
        let explicit = Cli::try_parse_from(["gproxy", "--master-key-rotate", "false"]).unwrap();
        assert_eq!(explicit.options.master_key_rotate.as_deref(), Some("false"));
        // And saying nothing is not the same as saying `false`.
        let silent = Cli::try_parse_from(["gproxy"]).unwrap();
        assert_eq!(silent.options.master_key_rotate, None);
    }

    #[test]
    fn the_bootstrap_flags_are_spelled_both_ways() {
        for argv in [
            vec!["gproxy", "bootstrap", "admin", "--user", "root"],
            vec!["gproxy", "bootstrap", "admin", "--admin-user", "root"],
        ] {
            let cli = Cli::try_parse_from(&argv).unwrap();
            assert_eq!(cli.options.admin_user.as_deref(), Some("root"), "{argv:?}");
        }
    }

    #[test]
    fn import_defaults_to_the_additive_mode() {
        let cli = Cli::try_parse_from(["gproxy", "import", "--in", "x.json"]).unwrap();
        let Some(Command::Import { mode, .. }) = cli.command else {
            panic!("not an import");
        };
        assert_eq!(mode, ImportModeArg::Merge);
    }

    #[test]
    fn an_import_names_one_document_or_the_other_and_never_both() {
        let v4 = Cli::try_parse_from(["gproxy", "import", "--in", "x.json"]).unwrap();
        let Some(Command::Import { input, from_v3, .. }) = v4.command else {
            panic!("not an import");
        };
        assert_eq!(input.as_deref(), Some(std::path::Path::new("x.json")));
        assert!(from_v3.is_none());

        let v3 = Cli::try_parse_from(["gproxy", "import", "--from-v3", "v3.json"]).unwrap();
        let Some(Command::Import { input, from_v3, .. }) = v3.command else {
            panic!("not an import");
        };
        assert!(input.is_none());
        assert_eq!(from_v3.as_deref(), Some(std::path::Path::new("v3.json")));

        // Neither is not an import, and both is two different documents.
        assert!(Cli::try_parse_from(["gproxy", "import"]).is_err());
        assert!(Cli::try_parse_from(["gproxy", "import", "--in", "a", "--from-v3", "b"]).is_err());
    }

    /// Every configurable value must be reachable from the environment, or the
    /// README's table and `--help` describe different programs. This asserts it
    /// against the parsed grammar rather than against a hand-kept list.
    #[test]
    fn every_option_names_an_environment_variable() {
        let command = Cli::command();
        let missing: Vec<_> = command
            .get_arguments()
            .filter(|arg| !arg.is_global_set() && arg.get_id() != "help")
            .map(|arg| arg.get_id().to_string())
            .collect();
        assert!(missing.is_empty(), "not global: {missing:?}");

        let without_env: Vec<_> = command
            .get_arguments()
            .filter(|arg| arg.is_global_set() && arg.get_env().is_none())
            .map(|arg| arg.get_id().to_string())
            .collect();
        assert!(
            without_env.is_empty(),
            "configuration without an environment name: {without_env:?}"
        );
    }
}
