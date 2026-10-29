use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Serialize)]
pub struct Commit {
    pub oid: String,
    pub timestamp: i64,
    pub date: String,
    pub subject: String,
}
#[derive(Clone, Debug, Serialize)]
pub struct FileState {
    pub oid: String,
    pub bytes: u64,
    pub born: usize,
    pub changed: usize,
    pub touches: u32,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub commit: Commit,
    pub files: BTreeMap<String, FileState>,
}
#[derive(Clone, Debug, Serialize)]
pub struct History {
    pub name: String,
    pub root: PathBuf,
    pub total_commits: usize,
    pub shallow: bool,
    pub snapshots: Vec<Snapshot>,
}
#[derive(Clone, Debug)]
pub struct ReadOptions {
    pub revision: String,
    pub since: Option<String>,
    pub until: Option<String>,
    pub max_commits: usize,
    pub exclude: Vec<String>,
    pub include_generated: bool,
    pub max_files: Option<usize>,
}
impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            revision: "HEAD".into(),
            since: None,
            until: None,
            max_commits: 240,
            exclude: vec![],
            include_generated: false,
            max_files: None,
        }
    }
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("LC_ALL", "C")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .context("Git is required. Install Git and retry.")?;
    if !out.status.success() {
        bail!(
            "Git failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

pub fn samples(length: usize, limit: usize) -> Vec<usize> {
    if length <= limit {
        return (0..length).collect();
    }
    if limit == 1 {
        return vec![length - 1];
    }
    (0..limit).map(|i| i * (length - 1) / (limit - 1)).collect()
}

fn exclusions(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        builder
            .add(Glob::new(pattern).with_context(|| format!("Invalid exclusion glob: {pattern}"))?);
    }
    Ok(builder.build()?)
}
fn ignored(path: &str, options: &ReadOptions, excludes: &GlobSet) -> bool {
    (!options.include_generated
        && path.split('/').any(|s| {
            matches!(
                s,
                "node_modules"
                    | "target"
                    | "vendor"
                    | "dist"
                    | "build"
                    | ".next"
                    | "__pycache__"
                    | ".venv"
            )
        }))
        || excludes.is_match(path)
}

fn parse_tree(bytes: &[u8]) -> Result<BTreeMap<String, (String, u64)>> {
    let mut files = BTreeMap::new();
    for row in bytes.split(|&c| c == 0).filter(|r| !r.is_empty()) {
        let tab = row
            .iter()
            .position(|&b| b == b'\t')
            .context("Invalid Git tree record")?;
        let meta = std::str::from_utf8(&row[..tab])?;
        let fields: Vec<_> = meta.split_whitespace().collect();
        if fields.len() != 4 || fields[1] != "blob" {
            continue;
        } // Submodule contents live in another repository.
        let path = std::str::from_utf8(&row[tab + 1..])
            .context("Gitarium currently requires UTF-8 file paths")?
            .to_owned();
        files.insert(path, (fields[2].to_owned(), fields[3].parse()?));
    }
    Ok(files)
}

pub fn read(root: &Path, options: &ReadOptions) -> Result<History> {
    if options.max_commits == 0 {
        bail!("--max-commits must be at least 1");
    }
    let root = root
        .canonicalize()
        .with_context(|| format!("Repository does not exist: {}", root.display()))?;
    // Resolve untrusted revisions to an object ID before passing them to other commands.
    let revision = format!("{}^{{commit}}", options.revision);
    let resolved = git(
        &root,
        &["rev-parse", "--verify", "--end-of-options", &revision],
    )?;
    let oid = std::str::from_utf8(&resolved)?.trim();
    let bytes = git(
        &root,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H%x00%ct%x00%cI%x00%s%x00",
            oid,
            "--",
        ],
    )?;
    let fields: Vec<_> = bytes.split(|&c| c == 0).collect();
    let mut commits = Vec::new();
    for row in fields.chunks(4) {
        if row.len() < 4 {
            continue;
        }
        let text = |i| String::from_utf8_lossy(row[i]).into_owned();
        let commit = Commit {
            oid: text(0).trim().into(),
            timestamp: text(1).parse()?,
            date: chrono::DateTime::from_timestamp(text(1).parse()?, 0)
                .context("Commit date is outside the supported range")?
                .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            subject: text(3),
        };
        let day = &commit.date[..10];
        if options.since.as_ref().is_some_and(|s| day < s.as_str())
            || options.until.as_ref().is_some_and(|s| day > s.as_str())
        {
            continue;
        }
        commits.push(commit);
    }
    if commits.is_empty() {
        bail!("No commits in the selected history or date range");
    }
    let total_commits = commits.len();
    let shallow = String::from_utf8_lossy(&git(&root, &["rev-parse", "--is-shallow-repository"])?)
        .trim()
        == "true";
    let excludes = exclusions(&options.exclude)?;
    let mut snapshots: Vec<Snapshot> = Vec::new();
    for index in samples(total_commits, options.max_commits) {
        let commit = commits[index].clone();
        let tree = parse_tree(&git(
            &root,
            &["ls-tree", "-r", "-l", "-z", &commit.oid, "--"],
        )?)?;
        let step = snapshots.len();
        let mut files = BTreeMap::new();
        for (path, (oid, size)) in tree {
            if ignored(&path, options, &excludes) {
                continue;
            }
            let old = snapshots.last().and_then(|s| s.files.get(&path));
            let state = match old {
                Some(old) if old.oid == oid => old.clone(),
                Some(old) => FileState {
                    oid,
                    bytes: size,
                    born: old.born,
                    changed: step,
                    touches: old.touches + 1,
                },
                None => FileState {
                    oid,
                    bytes: size,
                    born: step,
                    changed: step,
                    touches: 1,
                },
            };
            files.insert(path, state);
            if options.max_files.is_some_and(|limit| files.len() > limit) {
                bail!("Repository exceeds the configured file limit");
            }
        }
        snapshots.push(Snapshot { commit, files });
    }
    let name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("repository")
        .to_owned();
    Ok(History {
        name,
        root,
        total_commits,
        shallow,
        snapshots,
    })
}

impl History {
    pub fn union(&self) -> BTreeSet<String> {
        self.snapshots
            .iter()
            .flat_map(|s| s.files.keys().cloned())
            .collect()
    }
    pub fn summary(&self) -> serde_json::Value {
        let final_state = self.snapshots.last().expect("nonempty history");
        let mut species = BTreeMap::<&str, usize>::new();
        let mut dirs = BTreeSet::new();
        for path in final_state.files.keys() {
            *species
                .entry(crate::species::classify(path).name)
                .or_default() += 1;
            dirs.insert(Path::new(path).parent().unwrap_or(Path::new("")));
        }
        serde_json::json!({"repository": self.name, "root": self.root, "commits": self.total_commits,
            "snapshots": self.snapshots.len(), "shallow": self.shallow, "files": final_state.files.len(),
            "directories": dirs.len(), "species": species, "first_commit": self.snapshots[0].commit,
            "last_commit": final_state.commit, "unique_paths": self.union().len()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampling_preserves_endpoints() {
        assert_eq!(samples(100, 3), vec![0, 49, 99]);
        assert_eq!(samples(7, 1), vec![6]);
        assert_eq!(samples(2, 7), vec![0, 1]);
    }
    #[test]
    fn null_delimited_tree_keeps_unusual_names() {
        let tree = parse_tree(b"100644 blob abcd 12\todd\nfile\tname.rs\0").unwrap();
        assert_eq!(tree["odd\nfile\tname.rs"].1, 12);
    }
}
