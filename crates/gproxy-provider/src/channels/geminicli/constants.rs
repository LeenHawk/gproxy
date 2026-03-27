pub const DEFAULT_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
pub const DEFAULT_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
pub const DEFAULT_MANUAL_REDIRECT_URI: &str = "https://codeassist.google.com/authcode";
pub const DEFAULT_AUTHORIZATION_CODE_REDIRECT_URI: &str = "http://127.0.0.1:1455/oauth2callback";
pub const USERINFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";
pub const DEFAULT_UA_MODEL: &str = "gemini-2.5-pro";

pub const CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
pub const CLIENT_SECRET: &str = "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";
pub const OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile";
pub const OAUTH_STATE_TTL_MS: u64 = 600_000;
pub const TOKEN_REFRESH_SKEW_MS: u64 = 60_000;

pub fn geminicli_user_agent(model: Option<&str>) -> String {
    let model = model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(DEFAULT_UA_MODEL);
    format!(
        "GeminiCLI/{}/{model} ({}; {}; cloud-shell)",
        "0.35.2",
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}
