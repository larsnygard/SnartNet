use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

fn main() {
    // CI artifacts use the workflow run and attempt; local builds use build time.
    // Cargo's default change tracking reruns this when package sources change.
    let build = match (
        env::var("GITHUB_RUN_NUMBER"),
        env::var("GITHUB_RUN_ATTEMPT"),
    ) {
        (Ok(run), Ok(attempt)) => format!("{run}.{attempt}"),
        _ => SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("build clock must be after the Unix epoch")
            .as_secs()
            .to_string(),
    };
    println!("cargo:rustc-env=SNARTNET_BUILD={build}");
    println!(
        "cargo:rustc-env=SNARTNET_BUILD_PROFILE={}",
        env::var("PROFILE").unwrap()
    );
}
