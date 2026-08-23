use anyhow::{Context, Result, anyhow};
use moli_wpt_compat::{
    WptImportCopyConfig, WptImportDryRunConfig, copy_wpt_import, dry_run_wpt_import,
};
use std::path::{Path, PathBuf};

fn main() -> Result<()> {
    let args = ImportArgs::parse(std::env::args().skip(1))?;
    if !args.dry_run && !args.copy {
        return Err(anyhow!(
            "choose --dry-run or --copy; manifest draft generation is included in --copy output"
        ));
    }
    if args.dry_run && args.copy {
        return Err(anyhow!("choose only one of --dry-run or --copy"));
    }

    let source_commit = match args.source_commit {
        Some(commit) => commit,
        None => git_head(&args.wpt_root).unwrap_or_else(|| "unknown".to_owned()),
    };
    let dry_run = WptImportDryRunConfig {
        wpt_root: args.wpt_root,
        source: args.source,
        source_commit,
        target_suite: args.target_suite,
        extra_tags: args.extra_tags,
        paths: args.paths,
    };
    let output = if args.copy {
        serde_json::to_string_pretty(&copy_wpt_import(&WptImportCopyConfig {
            dry_run,
            fixture_root: args.fixture_root,
        })?)
        .context("failed to serialize WPT import report")?
    } else {
        serde_json::to_string_pretty(&dry_run_wpt_import(&dry_run)?)
            .context("failed to serialize WPT import report")?
    };
    println!("{output}");
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ImportArgs {
    dry_run: bool,
    copy: bool,
    wpt_root: PathBuf,
    fixture_root: PathBuf,
    source: String,
    source_commit: Option<String>,
    target_suite: String,
    extra_tags: Vec<String>,
    paths: Vec<String>,
}

impl ImportArgs {
    fn parse(args: impl IntoIterator<Item = String>) -> Result<Self> {
        let mut dry_run = false;
        let mut copy = false;
        let mut wpt_root = PathBuf::from("../wpt");
        let mut fixture_root = PathBuf::from("moli-wpt-compat/fixtures/wpt");
        let mut source = "upstream-wpt".to_owned();
        let mut source_commit = None;
        let mut target_suite = "broad".to_owned();
        let mut extra_tags = Vec::new();
        let mut paths = Vec::new();

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--dry-run" => dry_run = true,
                "--copy" => copy = true,
                "--wpt-root" => {
                    wpt_root = PathBuf::from(
                        args.next()
                            .ok_or_else(|| anyhow!("--wpt-root requires a value"))?,
                    );
                }
                "--fixture-root" => {
                    fixture_root = PathBuf::from(
                        args.next()
                            .ok_or_else(|| anyhow!("--fixture-root requires a value"))?,
                    );
                }
                "--source" => {
                    source = args
                        .next()
                        .ok_or_else(|| anyhow!("--source requires a value"))?;
                }
                "--source-commit" => {
                    source_commit = Some(
                        args.next()
                            .ok_or_else(|| anyhow!("--source-commit requires a value"))?,
                    );
                }
                "--suite" => {
                    target_suite = args
                        .next()
                        .ok_or_else(|| anyhow!("--suite requires a value"))?;
                }
                "--tag" => {
                    let value = args
                        .next()
                        .ok_or_else(|| anyhow!("--tag requires a value"))?;
                    append_tags(&mut extra_tags, &value)?;
                }
                "--help" | "-h" => return Err(anyhow!(usage())),
                other if other.starts_with('-') => {
                    return Err(anyhow!("unsupported argument '{other}'\n{}", usage()));
                }
                path => paths.push(path.to_owned()),
            }
        }

        if paths.is_empty() {
            return Err(anyhow!("no upstream WPT paths provided\n{}", usage()));
        }

        Ok(Self {
            dry_run,
            copy,
            wpt_root,
            fixture_root,
            source,
            source_commit,
            target_suite,
            extra_tags,
            paths,
        })
    }
}

fn append_tags(tags: &mut Vec<String>, value: &str) -> Result<()> {
    let parsed = value
        .split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    if parsed.is_empty() {
        return Err(anyhow!("--tag requires at least one non-empty tag"));
    }
    for tag in parsed {
        if !tags.iter().any(|existing| existing == tag) {
            tags.push(tag.to_owned());
        }
    }
    Ok(())
}

fn git_head(root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8(output.stdout).ok()?;
    let commit = commit.trim();
    if commit.is_empty() {
        None
    } else {
        Some(commit.to_owned())
    }
}

fn usage() -> &'static str {
    "usage: cargo run -p moli-wpt-compat --bin wpt_import -- (--dry-run | --copy) [--wpt-root ../wpt] [--fixture-root moli-wpt-compat/fixtures/wpt] [--suite broad] [--source upstream-wpt] [--source-commit <sha>] [--tag <tag>[,<tag>...]] <path>..."
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_dry_run_and_paths() -> Result<()> {
        let args = ImportArgs::parse([
            "--dry-run".to_owned(),
            "--wpt-root".to_owned(),
            "../wpt".to_owned(),
            "--suite".to_owned(),
            "experimental".to_owned(),
            "--source-commit".to_owned(),
            "abc".to_owned(),
            "--tag".to_owned(),
            "real-layout-behavior,harness-blocked,explicit-probe".to_owned(),
            "url".to_owned(),
        ])?;

        assert!(args.dry_run);
        assert!(!args.copy);
        assert_eq!(args.wpt_root, PathBuf::from("../wpt"));
        assert_eq!(
            args.fixture_root,
            PathBuf::from("moli-wpt-compat/fixtures/wpt")
        );
        assert_eq!(args.target_suite, "experimental");
        assert_eq!(args.source_commit.as_deref(), Some("abc"));
        assert_eq!(
            args.extra_tags,
            ["real-layout-behavior", "harness-blocked", "explicit-probe"]
        );
        assert_eq!(args.paths, ["url"]);
        Ok(())
    }

    #[test]
    fn parse_rejects_missing_paths() {
        let error =
            ImportArgs::parse(["--dry-run".to_owned()]).expect_err("missing paths should fail");
        assert!(error.to_string().contains("no upstream WPT paths"));
    }

    #[test]
    fn parse_accepts_copy_and_fixture_root() -> Result<()> {
        let args = ImportArgs::parse([
            "--copy".to_owned(),
            "--fixture-root".to_owned(),
            "fixtures".to_owned(),
            "url".to_owned(),
        ])?;

        assert!(args.copy);
        assert!(!args.dry_run);
        assert_eq!(args.fixture_root, PathBuf::from("fixtures"));
        assert_eq!(args.paths, ["url"]);
        Ok(())
    }

    #[test]
    fn parse_rejects_empty_tag() {
        let error = ImportArgs::parse([
            "--dry-run".to_owned(),
            "--tag".to_owned(),
            ",".to_owned(),
            "url".to_owned(),
        ])
        .expect_err("empty tag should fail");
        assert!(error.to_string().contains("--tag requires"));
    }
}
