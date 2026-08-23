use std::{
    path::PathBuf,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;

fn moli_cli_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_moli"))
}

fn unique_temp_dir(name: &str) -> Result<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let dir = std::env::temp_dir().join(format!("moli-{name}-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn run_fetch_in_dir(working_dir: &std::path::Path) -> Result<Output> {
    Ok(Command::new(moli_cli_path())
        .current_dir(working_dir)
        .arg("fetch")
        .arg("--log-level")
        .arg("error")
        .arg("about:blank")
        .env_remove("HTTP_PROXY")
        .env_remove("HTTPS_PROXY")
        .env_remove("ALL_PROXY")
        .env_remove("http_proxy")
        .env_remove("https_proxy")
        .env_remove("all_proxy")
        .output()?)
}

#[test]
fn cli_fetch_without_profile_dir_does_not_create_moli_profile_dir() -> Result<()> {
    let working_dir = unique_temp_dir("fresh-fetch-cwd")?;

    let output = run_fetch_in_dir(&working_dir)?;

    assert!(
        output.status.success(),
        "fresh fetch failed: stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !working_dir.join(".moli").exists(),
        "no-profile fetch should not create {}",
        working_dir.join(".moli").display()
    );
    Ok(())
}
