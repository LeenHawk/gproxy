use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=GPROXY_BUILD_HASH");
    if std::env::var_os("GPROXY_BUILD_HASH").is_some() {
        return;
    }
    let Ok(output) = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
    else {
        return;
    };
    if output.status.success()
        && let Ok(hash) = std::str::from_utf8(&output.stdout)
    {
        println!("cargo:rustc-env=GPROXY_BUILD_HASH={}", hash.trim());
    }
}
