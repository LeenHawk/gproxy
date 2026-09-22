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
}
