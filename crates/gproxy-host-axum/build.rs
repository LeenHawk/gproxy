use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=GPROXY_BUILD_HASH");
    if std::env::var_os("GPROXY_BUILD_HASH").is_some() {
        return;
    }
    if let Some(head) = git(&["rev-parse", "--path-format=absolute", "--git-path", "HEAD"]) {
        println!("cargo:rerun-if-changed={head}");
    }
    if let Some(reference) = git(&["symbolic-ref", "-q", "HEAD"])
        && let Some(path) = git(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            &reference,
        ])
    {
        println!("cargo:rerun-if-changed={path}");
    }
    if let Some(hash) = git(&["rev-parse", "--short=12", "HEAD"]) {
        println!("cargo:rustc-env=GPROXY_BUILD_HASH={hash}");
    }
}

fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8(output.stdout).ok()?.trim().to_owned())
}
