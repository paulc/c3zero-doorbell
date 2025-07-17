use std::env;
use std::process::Command;

fn main() {
    embuild::espidf::sysenv::output();

    // Build information
    let ts =
        time_format::strftime_local("%Y-%m-%d %H:%M:%S %Z", time_format::now().unwrap()).unwrap();
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();
    let short_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .unwrap_or_else(|| "unknown".to_string())
        .trim()
        .to_string();

    println!("cargo:rustc-env=BUILD_TS={ts}");
    println!("cargo:rustc-env=BUILD_BRANCH={branch}");
    println!("cargo:rustc-env=BUILD_HASH={short_hash}");
    println!(
        "cargo:rustc-env=BUILD_PROFILE={}",
        env::var("PROFILE").unwrap()
    );
}
