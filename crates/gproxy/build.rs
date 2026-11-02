fn main() {
    println!("cargo:rerun-if-env-changed=GPROXY_BUILD_HASH");
    let hash = std::env::var("GPROXY_BUILD_HASH")
        .ok()
        .or_else(|| {
            let output = std::process::Command::new("git")
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()?;
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        })
        .unwrap_or_else(|| "unknown".to_owned());
    for name in ["HEAD", "logs/HEAD"] {
        if let Ok(output) = std::process::Command::new("git")
            .args(["rev-parse", "--git-path", name])
            .output()
            && output.status.success()
        {
            println!(
                "cargo:rerun-if-changed={}",
                String::from_utf8_lossy(&output.stdout).trim()
            );
        }
    }
    println!("cargo:rustc-env=GPROXY_BUILD_HASH={hash}");
    // For `--version`: which commit, for which machine, how optimised, and
    // which of this crate's features — enough to identify a pasted build.
    let short: String = hash.chars().take(12).collect();
    println!("cargo:rustc-env=GPROXY_BUILD_SHORT_HASH={short}");
    for name in ["TARGET", "PROFILE"] {
        let value = std::env::var(name).unwrap_or_else(|_| "unknown".to_owned());
        println!("cargo:rustc-env=GPROXY_BUILD_{name}={value}");
    }
    let features = std::env::var("CARGO_CFG_FEATURE").unwrap_or_default();
    let features = if features.is_empty() {
        "none".to_owned()
    } else {
        features.replace(',', ", ")
    };
    println!("cargo:rustc-env=GPROXY_BUILD_FEATURES={features}");
}
