use gitarium::{
    cli,
    history::{self, ReadOptions},
    layout::Layout,
    render::Renderer,
    video::{self, VideoOptions},
};
use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Gardener")
        .env("GIT_AUTHOR_EMAIL", "gardener@example.test")
        .env("GIT_COMMITTER_NAME", "Gardener")
        .env("GIT_COMMITTER_EMAIL", "gardener@example.test")
        .env("GIT_AUTHOR_DATE", "2025-01-02T12:00:00Z")
        .env("GIT_COMMITTER_DATE", "2025-01-02T12:00:00Z")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().into()
}
fn repo() -> TempDir {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-b", "main"]);
    dir
}
fn write(root: &Path, path: &str, value: &str) {
    let p = root.join(path);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, value).unwrap();
}
fn commit(root: &Path, message: &str) {
    git(root, &["add", "--all"]);
    git(
        root,
        &["-c", "core.hooksPath=/dev/null", "commit", "-m", message],
    );
}

#[test]
fn exact_trees_track_edits_removal_rename_and_merge() {
    let dir = repo();
    let root = dir.path();
    write(root, "src/main.rs", "one");
    write(root, "odd\nname\t.ts", "hello");
    commit(root, "seed");
    write(root, "src/main.rs", "two");
    commit(root, "edit");
    git(root, &["switch", "-c", "feature"]);
    write(root, "flower.py", "bloom");
    commit(root, "branch");
    git(root, &["switch", "main"]);
    write(root, "README.md", "garden");
    commit(root, "docs");
    git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "merge",
            "--no-ff",
            "feature",
            "-m",
            "merge flowers",
        ],
    );
    git(root, &["mv", "src/main.rs", "src/world.rs"]);
    fs::remove_file(root.join("odd\nname\t.ts")).unwrap();
    commit(root, "transplant");
    let history = history::read(root, &ReadOptions::default()).unwrap();
    assert_eq!(history.total_commits, 5); // The feature commit enters through the merge tree.
    assert_eq!(history.snapshots[1].files["src/main.rs"].touches, 2);
    assert_eq!(history.snapshots[1].files["src/main.rs"].born, 0);
    assert!(history.snapshots[0].files.contains_key("odd\nname\t.ts"));
    assert!(!history.snapshots[2].files.contains_key("flower.py"));
    assert!(history.snapshots[3].files.contains_key("flower.py"));
    let final_state = &history.snapshots.last().unwrap().files;
    assert_eq!(final_state.len(), 3);
    assert!(final_state.contains_key("src/world.rs"));
    assert!(!final_state.contains_key("src/main.rs"));
    let sampled = history::read(
        root,
        &ReadOptions {
            max_commits: 2,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(
        sampled
            .snapshots
            .last()
            .unwrap()
            .files
            .keys()
            .collect::<Vec<_>>(),
        final_state.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        sampled.snapshots[0].commit.oid,
        history.snapshots[0].commit.oid
    );
}

#[test]
fn filters_and_empty_tree_work_without_changing_repository() {
    let dir = repo();
    let root = dir.path();
    git(
        root,
        &[
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--allow-empty",
            "-m",
            "empty",
        ],
    );
    write(root, "src/a.rs", "fern");
    write(root, "target/debug/a.rs", "generated");
    write(root, "image.png", "lily");
    commit(root, "grow");
    let before = git(root, &["status", "--porcelain=v1"]);
    let history = history::read(
        root,
        &ReadOptions {
            exclude: vec!["*.png".into()],
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert!(history.snapshots[0].files.is_empty());
    assert_eq!(history.snapshots[1].files.len(), 1);
    let included = history::read(
        root,
        &ReadOptions {
            include_generated: true,
            ..ReadOptions::default()
        },
    )
    .unwrap();
    assert_eq!(included.snapshots[1].files.len(), 3);
    assert_eq!(git(root, &["status", "--porcelain=v1"]), before);
    assert!(
        history::read(
            root,
            &ReadOptions {
                revision: "--all".into(),
                ..ReadOptions::default()
            }
        )
        .is_err()
    );
}

#[test]
fn render_is_deterministic_and_obeys_plant_budget() {
    let history = cli::demo();
    let layout = Layout::new(&history, 2, 18, 3);
    assert_eq!(
        layout
            .snapshots
            .last()
            .unwrap()
            .iter()
            .map(|i| i.plants.len())
            .sum::<usize>(),
        3
    );
    assert_eq!(
        layout.hidden_plants + 3,
        history.snapshots.last().unwrap().files.len()
    );
    let mut renderer = Renderer::new(layout).unwrap();
    let a = renderer.frame(&history, 47.0, 0.0, 640, 360);
    let b = renderer.frame(&history, 47.0, 0.0, 640, 360);
    assert_eq!(a, b);
    assert_eq!(a.dimensions(), (640, 360));
    assert_ne!(a, renderer.frame(&history, 0.0, 0.0, 640, 360));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garden.png");
    video::save_png(&a, &path, false).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(video::save_png(&b, &path, false).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
}

#[test]
fn cli_json_schema_dry_run_and_validation() {
    let binary = env!("CARGO_BIN_EXE_gitarium");
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("no-directory/movie.mp4");
    let out = Command::new(binary)
        .args(["demo", "-o"])
        .arg(&output)
        .arg("--dry-run")
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["ok"], true);
    assert_eq!(json["data"]["dry_run"], true);
    assert!(!output.parent().unwrap().exists());
    for args in [
        vec!["demo", "-o", "x.mp4", "--duration", "NaN"],
        vec!["demo", "-o", "x.mp4", "--fps", "0"],
        vec!["snapshot", "-o", "x.png", "--since", "2025-02-30"],
    ] {
        let out = Command::new(binary).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(json["ok"], false);
    }
    let out = Command::new(binary)
        .args(["schema", "export"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        json["data"]["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["long"] == "duration")
    );
}

#[test]
fn calendar_handles_backwards_clocks_and_video_constraints() {
    let mut h = cli::demo();
    h.snapshots[3].commit.timestamp = h.snapshots[1].commit.timestamp;
    let positions: Vec<_> = (0..100)
        .map(|i| video::calendar_position(&h, i as f32 / 99.0))
        .collect();
    assert!(positions.windows(2).all(|p| p[0] <= p[1]));
    assert_eq!(*positions.last().unwrap(), 47.0);
    assert!(
        VideoOptions {
            width: 641,
            height: 360,
            fps: 12,
            duration: 3.0,
            hold: 1.0,
            calendar: false,
            transition: 1.2,
        }
        .validate()
        .is_err()
    );
}

#[test]
fn encoder_roundtrip_when_available() {
    let Ok(encoder) = std::env::var("GITARIUM_TEST_FFMPEG") else {
        return;
    };
    let history = cli::demo();
    let mut renderer = Renderer::new(Layout::new(&history, 2, 18, 150)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garden.mp4");
    let frames = video::export(
        &history,
        &mut renderer,
        &VideoOptions {
            width: 640,
            height: 360,
            fps: 12,
            duration: 2.0,
            hold: 0.5,
            calendar: false,
            transition: 1.2,
        },
        &path,
        Path::new(&encoder),
        false,
    )
    .unwrap();
    assert_eq!(frames, 24);
    assert!(fs::metadata(&path).unwrap().len() > 10000);
    let out = Command::new(&encoder)
        .args(["-v", "error", "-i"])
        .arg(&path)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn demo_covers_the_complete_botanical_catalog() {
    let history = cli::demo();
    let species: std::collections::BTreeSet<_> = history
        .snapshots
        .last()
        .unwrap()
        .files
        .keys()
        .map(|path| gitarium::species::classify(path).id)
        .collect();
    assert_eq!(species.len(), gitarium::species::SPECIES.len());
}

fn synthetic_history(states: Vec<Vec<String>>) -> history::History {
    use history::{Commit, FileState, Snapshot};
    history::History {
        name: "nested".into(),
        root: "fixture".into(),
        total_commits: states.len(),
        shallow: false,
        snapshots: states
            .into_iter()
            .enumerate()
            .map(|(step, paths)| Snapshot {
                commit: Commit {
                    oid: format!("{step:040x}"),
                    timestamp: 1735689600 + step as i64,
                    date: "2025-01-01T00:00:00Z".into(),
                    subject: "grow".into(),
                },
                files: paths
                    .into_iter()
                    .map(|p| {
                        (
                            p,
                            FileState {
                                oid: "blob".into(),
                                bytes: 100,
                                born: 0,
                                changed: 0,
                                touches: 1,
                            },
                        )
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[test]
fn threshold_promotes_and_retires_children_without_double_counting() {
    let initial: Vec<String> = [
        "README.md",
        "src/root.rs",
        "src/api/a.rs",
        "src/api/b.rs",
        "src/api/c.rs",
    ]
    .iter()
    .map(|p| p.to_string())
    .collect();
    let mut second = initial.clone();
    second.push("src/api/d.rs".into());
    let mut third = second.clone();
    third.push("src/api/e.rs".into());
    let final_state = vec![
        "README.md".into(),
        "src/root.rs".into(),
        "src/api/a.rs".into(),
    ];
    let h = synthetic_history(vec![initial, second, third, final_state]);
    let layout = Layout::with_threshold(&h, 6, 32, 100, Some(4));
    let src = layout
        .islands
        .iter()
        .position(|n| n.directory == "src")
        .unwrap();
    let api = layout
        .islands
        .iter()
        .position(|n| n.directory == "src/api")
        .unwrap();
    assert_eq!(layout.islands[api].parent, Some(src));
    assert_eq!(layout.islands[src].parent, Some(0));
    assert!(!layout.snapshots[0][src].active); // Exactly N is still part of its parent.
    assert!(layout.snapshots[1][src].active);
    assert!(!layout.snapshots[1][api].active);
    assert!(layout.snapshots[2][api].active);
    assert_eq!(layout.snapshots[2][api].file_count, 5);
    assert_eq!(layout.snapshots[2][src].file_count, 1);
    assert_eq!(layout.snapshots[2][0].file_count, 1);
    assert!(!layout.snapshots[3][api].active);
    assert!(!layout.snapshots[3][src].active);
    for (snapshot, states) in h.snapshots.iter().zip(&layout.snapshots) {
        assert_eq!(
            states.iter().map(|n| n.file_count).sum::<usize>(),
            snapshot.files.len()
        );
        let plants: Vec<_> = states.iter().flat_map(|n| &n.plants).collect();
        assert_eq!(
            plants
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            plants.len()
        );
        assert!(plants.iter().all(|p| snapshot.files.contains_key(*p)));
    }
    assert_eq!(layout.bridges.len(), layout.islands.len() - 1);
    for bridge in &layout.bridges {
        assert!(layout.islands[bridge.child].radius < layout.islands[bridge.parent].radius);
        assert_eq!(layout.islands[bridge.child].parent, Some(bridge.parent));
        assert!(bridge.sprite < 16);
    }
}

#[test]
fn historical_path_churn_does_not_falsely_pass_the_threshold() {
    let h = synthetic_history(vec![
        (0..4).map(|i| format!("temporary/{i}.rs")).collect(),
        (4..8).map(|i| format!("temporary/{i}.rs")).collect(),
    ]);
    let layout = Layout::with_threshold(&h, 6, 32, 100, Some(4));
    assert_eq!(layout.islands.len(), 1);
    assert!(layout.bridges.is_empty());
}

#[test]
fn island_limit_keeps_a_connected_tree_and_the_file_budget() {
    let paths = (0..12)
        .flat_map(|i| (0..6).map(move |j| format!("src/module{i}/nested/file{j}.rs")))
        .collect();
    let h = synthetic_history(vec![paths]);
    let layout = Layout::with_threshold(&h, 6, 8, 7, Some(4));
    assert_eq!(layout.islands.len(), 8);
    assert_eq!(layout.bridges.len(), 7);
    for (i, island) in layout.islands.iter().enumerate().skip(1) {
        let parent = island.parent.unwrap();
        assert!(parent < i);
        assert!(
            island
                .directory
                .starts_with(&layout.islands[parent].directory)
                || parent == 0
        );
    }
    assert_eq!(
        layout.snapshots[0]
            .iter()
            .map(|n| n.file_count)
            .sum::<usize>(),
        72
    );
    assert_eq!(
        layout.snapshots[0]
            .iter()
            .map(|n| n.plants.len())
            .sum::<usize>(),
        7
    );
}

#[test]
fn history_file_limit_stops_oversized_snapshots() {
    let dir = repo();
    write(dir.path(), "a.rs", "one");
    write(dir.path(), "b.rs", "two");
    commit(dir.path(), "grow");
    let error = history::read(
        dir.path(),
        &ReadOptions {
            max_files: Some(1),
            ..ReadOptions::default()
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("file limit"));
}

#[test]
fn gif_roundtrip_when_available() {
    let Ok(encoder) = std::env::var("GITARIUM_TEST_FFMPEG") else {
        return;
    };
    let history = cli::demo();
    let mut renderer = Renderer::new(Layout::new(&history, 6, 24, 150)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("garden.gif");
    let frames = video::export(
        &history,
        &mut renderer,
        &VideoOptions {
            width: 640,
            height: 360,
            fps: 12,
            duration: 3.0,
            hold: 0.5,
            calendar: false,
            transition: 1.2,
        },
        &path,
        Path::new(&encoder),
        false,
    )
    .unwrap();
    assert_eq!(frames, 36);
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(b"GIF89a") || bytes.starts_with(b"GIF87a"));
    let output = Command::new(encoder)
        .args(["-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
