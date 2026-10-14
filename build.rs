use std::process::Command;

fn main() {
    // A stamp for this build. `weft --version` shows it, and a running daemon is compared against
    // it: typing `weft` with a freshly installed binary replaces the daemon of an older build.
    let stamp = Command::new("date")
        .args(["-u", "+%Y-%m-%d %H:%M:%S UTC"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into());
    println!("cargo:rustc-env=WEFT_BUILD={stamp}");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=web");
}
