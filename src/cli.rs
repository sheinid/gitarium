use crate::{
    history::{self, History, ReadOptions},
    layout::Layout,
    render::Renderer,
    species::{SHORE_ASSETS, SPECIES},
    video::{self, VideoOptions},
};
use anyhow::{Result, bail};
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::{Value, json};
use std::{io::IsTerminal, path::PathBuf};

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Format {
    Auto,
    Json,
    Human,
}
#[derive(Parser, Debug)]
#[command(
    name = "gitarium",
    version,
    about = "Grow a pixel-art island garden from Git history.",
    after_help = "Start here: gitarium demo -o garden.mp4\nYour repository: gitarium export ~/code/repo -o repo.mp4\nStill image: gitarium snapshot . -o garden.png"
)]
pub struct Cli {
    #[arg(long, global = true, value_enum, default_value = "auto")]
    pub format: Format,
    #[command(subcommand)]
    pub command: Commands,
}
#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Render committed history to MP4 or an animated GIF. Writes the requested output file.
    Export(Export),
    /// Render a committed repository state to PNG. Writes the requested output file.
    Snapshot(Snapshot),
    /// Summarize a repository and its file species without writing files.
    Inspect(Source),
    /// Render the bundled 48-observation example, with no Git repository required.
    Demo(Demo),
    /// List botanical species and their file associations.
    Species,
    /// Print a Pages image URL, Markdown, and a first-render request link.
    Embed {
        /// OWNER/REPOSITORY or its GitHub URL.
        repository: String,
        #[arg(long, default_value=crate::embed::DEFAULT_BASE_URL)]
        base_url: String,
    },
    /// Check Git and the H.264 video encoder.
    Doctor {
        #[arg(long)]
        ffmpeg: Option<PathBuf>,
    },
    /// Describe a command and its typed arguments as JSON, derived from CLI definitions.
    Schema { command: Option<String> },
}
#[derive(Args, Debug)]
pub struct Source {
    /// Repository directory. The working tree is never modified.
    #[arg(default_value = ".")]
    pub repository: PathBuf,
    /// Branch, tag, or commit to follow through first-parent history.
    #[arg(long, default_value = "HEAD")]
    pub revision: String,
    /// First included commit date, YYYY-MM-DD, inclusive.
    #[arg(long, value_parser=date)]
    pub since: Option<String>,
    /// Last included commit date, YYYY-MM-DD, inclusive.
    #[arg(long, value_parser=date)]
    pub until: Option<String>,
    /// Maximum evenly sampled snapshots. Always includes the latest state.
    #[arg(long, default_value_t=240, value_parser=clap::value_parser!(u32).range(1..=2000))]
    pub max_commits: u32,
    /// Exclude repository-relative globs. May be repeated, quote '**/*.png'.
    #[arg(long)]
    pub exclude: Vec<String>,
    /// Include tracked node_modules, target, dist, build, and other generated directories.
    #[arg(long)]
    pub include_generated: bool,
    /// Stop if an observed snapshot exceeds this filtered file count.
    #[arg(long, value_parser=clap::value_parser!(u32).range(1..=1000000))]
    pub max_files: Option<u32>,
}
#[derive(Args, Debug)]
pub struct View {
    /// Maximum directory depth eligible for a child island.
    #[arg(long, default_value_t=6, value_parser=clap::value_parser!(u32).range(1..=12))]
    pub depth: u32,
    #[arg(long, default_value_t=32, value_parser=clap::value_parser!(u32).range(1..=40))]
    pub max_islands: u32,
    /// Create a child island when its subtree has more than N files. Omit for automatic N.
    #[arg(long, value_name="N", value_parser=clap::value_parser!(u32).range(1..=100000))]
    pub island_threshold: Option<u32>,
    /// Maximum visible plant budget. Counts still include all filtered files.
    #[arg(long, default_value_t=700, value_parser=clap::value_parser!(u32).range(1..=3000))]
    pub max_plants: u32,
    #[arg(long, default_value_t=1280, value_parser=clap::value_parser!(u32).range(320..=3840))]
    pub width: u32,
    #[arg(long, default_value_t=720, value_parser=clap::value_parser!(u32).range(180..=2160))]
    pub height: u32,
}
#[derive(Args, Debug)]
pub struct Movie {
    #[arg(long, default_value_t=20.0, value_parser=duration)]
    pub duration: f32,
    #[arg(long, default_value_t=24, value_parser=clap::value_parser!(u32).range(1..=60))]
    pub fps: u32,
    /// Seconds to linger on the final state, included in duration.
    #[arg(long, default_value_t=2.0, value_parser=finite)]
    pub hold: f32,
    /// Real-time entrance and exit duration, independent of commit playback speed.
    #[arg(long, default_value_t=1.2, value_parser=transition)]
    pub transition: f32,
    /// Pace by elapsed commit time, including quiet periods.
    #[arg(long)]
    pub calendar: bool,
    /// FFmpeg executable override. Also accepts GITARIUM_FFMPEG.
    #[arg(long)]
    pub ffmpeg: Option<PathBuf>,
}
#[derive(Args, Debug)]
pub struct Output {
    #[arg(short, long)]
    pub output: PathBuf,
    /// Replace an existing output file, after rendering succeeds.
    #[arg(long)]
    pub force: bool,
    /// Validate and describe the planned render without writing or encoding.
    #[arg(long)]
    pub dry_run: bool,
}
#[derive(Args, Debug)]
pub struct Export {
    #[command(flatten)]
    pub source: Source,
    #[command(flatten)]
    pub view: View,
    #[command(flatten)]
    pub movie: Movie,
    #[command(flatten)]
    pub output: Output,
}
#[derive(Args, Debug)]
pub struct Snapshot {
    #[command(flatten)]
    pub source: Source,
    #[command(flatten)]
    pub view: View,
    #[command(flatten)]
    pub output: Output,
    /// Point along the sampled history, from 0 to 1.
    #[arg(long, default_value_t=1.0, value_parser=progress)]
    pub progress: f32,
}
#[derive(Args, Debug)]
pub struct Demo {
    #[command(flatten)]
    pub view: View,
    #[command(flatten)]
    pub movie: Movie,
    #[command(flatten)]
    pub output: Output,
}
fn finite(input: &str) -> std::result::Result<f32, String> {
    let v: f32 = input.parse().map_err(|_| "Expected a number".to_string())?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err("Expected a finite number".into())
    }
}
fn duration(input: &str) -> std::result::Result<f32, String> {
    let v = finite(input)?;
    if (1.0..=600.0).contains(&v) {
        Ok(v)
    } else {
        Err("Expected 1..600 seconds".into())
    }
}
fn transition(input: &str) -> std::result::Result<f32, String> {
    let v = finite(input)?;
    if (0.1..=5.0).contains(&v) {
        Ok(v)
    } else {
        Err("Expected 0.1..5 seconds".into())
    }
}
fn progress(input: &str) -> std::result::Result<f32, String> {
    let v = finite(input)?;
    if (0.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err("Expected a value from 0 to 1".into())
    }
}
fn date(input: &str) -> std::result::Result<String, String> {
    if input.len() != 10 || chrono::NaiveDate::parse_from_str(input, "%Y-%m-%d").is_err() {
        return Err("Expected a valid YYYY-MM-DD date".into());
    }
    Ok(input.into())
}
impl Source {
    fn read(&self) -> Result<History> {
        if self
            .since
            .as_ref()
            .zip(self.until.as_ref())
            .is_some_and(|(s, u)| s > u)
        {
            bail!("--since must not be later than --until");
        }
        history::read(
            &self.repository,
            &ReadOptions {
                revision: self.revision.clone(),
                since: self.since.clone(),
                until: self.until.clone(),
                max_commits: self.max_commits as usize,
                exclude: self.exclude.clone(),
                include_generated: self.include_generated,
                max_files: self.max_files.map(|n| n as usize),
            },
        )
    }
}
impl View {
    fn layout(&self, history: &History) -> Layout {
        Layout::with_threshold(
            history,
            self.depth as usize,
            self.max_islands as usize,
            self.max_plants as usize,
            self.island_threshold.map(|n| n as usize),
        )
    }
    fn renderer(&self, history: &History) -> Result<Renderer> {
        Renderer::new(self.layout(history))
    }
}
impl Movie {
    fn options(&self, view: &View) -> Result<VideoOptions> {
        let options = VideoOptions {
            width: view.width,
            height: view.height,
            fps: self.fps,
            duration: self.duration,
            hold: self.hold,
            calendar: self.calendar,
            transition: self.transition,
        };
        options.validate()?;
        Ok(options)
    }
}
fn output_extension(output: &Output, ext: &str) -> Result<()> {
    if output
        .output
        .extension()
        .and_then(|s| s.to_str())
        .is_none_or(|s| !s.eq_ignore_ascii_case(ext))
    {
        bail!("Output must have a .{ext} extension");
    }
    if output.output.exists() && !output.force {
        bail!("Output already exists. Pass --force to replace it.");
    }
    Ok(())
}

fn movie(history: &History, view: &View, movie: &Movie, output: &Output) -> Result<Value> {
    let extension = output
        .output
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if !extension.eq_ignore_ascii_case("mp4") && !extension.eq_ignore_ascii_case("gif") {
        bail!("Output must have a .mp4 or .gif extension");
    }
    output_extension(output, extension)?;
    let options = movie.options(view)?;
    if output.dry_run {
        return Ok(
            json!({"dry_run":true,"output":output.output,"video":options,"history":history.summary(),"layout":view.layout(history).summary()}),
        );
    }
    let encoder = video::ffmpeg(movie.ffmpeg.as_deref())?;
    let mut renderer = view.renderer(history)?;
    let frames = video::export(
        history,
        &mut renderer,
        &options,
        &output.output,
        &encoder,
        output.force,
    )?;
    Ok(
        json!({"output":output.output.canonicalize()?,"frames":frames,"duration_seconds":frames as f32/options.fps as f32,"width":view.width,"height":view.height,"history":history.summary(),"layout":renderer.layout.summary()}),
    )
}

pub fn execute(cli: &Cli) -> Result<Value> {
    match &cli.command {
        Commands::Export(args) => {
            movie(&args.source.read()?, &args.view, &args.movie, &args.output)
        }
        Commands::Snapshot(args) => {
            output_extension(&args.output, "png")?;
            let history = args.source.read()?;
            if args.output.dry_run {
                return Ok(
                    json!({"dry_run":true,"output":args.output.output,"width":args.view.width,"height":args.view.height,"progress":args.progress,"history":history.summary(),"layout":args.view.layout(&history).summary()}),
                );
            }
            let mut renderer = args.view.renderer(&history)?;
            let frame = renderer.frame(
                &history,
                args.progress * (history.snapshots.len() - 1) as f32,
                0.0,
                args.view.width,
                args.view.height,
            );
            video::save_png(&frame, &args.output.output, args.output.force)?;
            Ok(
                json!({"output":args.output.output.canonicalize()?,"history":history.summary(),"layout":renderer.layout.summary()}),
            )
        }
        Commands::Inspect(source) => Ok(source.read()?.summary()),
        Commands::Embed {
            repository,
            base_url,
        } => crate::embed::Repository::parse(repository)?.markdown(base_url),
        Commands::Demo(args) => {
            let history = demo();
            if args
                .output
                .output
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                output_extension(&args.output, "png")?;
                if args.output.dry_run {
                    return Ok(
                        json!({"dry_run":true,"output":args.output.output,"history":history.summary(),"layout":args.view.layout(&history).summary()}),
                    );
                }
                let mut renderer = args.view.renderer(&history)?;
                let image = renderer.frame(
                    &history,
                    (history.snapshots.len() - 1) as f32,
                    0.0,
                    args.view.width,
                    args.view.height,
                );
                video::save_png(&image, &args.output.output, args.output.force)?;
                Ok(
                    json!({"output":args.output.output.canonicalize()?,"history":history.summary(),"layout":renderer.layout.summary()}),
                )
            } else {
                movie(&history, &args.view, &args.movie, &args.output)
            }
        }
        Commands::Species => Ok(
            json!({"species":SPECIES,"shore_assets":SHORE_ASSETS,"bridge_variants":16,"atlas":"32 plants, 16 shoreline assets, 16 bridges"}),
        ),
        Commands::Doctor { ffmpeg } => {
            let git = std::process::Command::new("git")
                .arg("--version")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
            let encoder = video::ffmpeg(ffmpeg.as_deref());
            Ok(
                json!({"git":git,"png_ready":true,"mp4_ready":encoder.is_ok(),"ffmpeg":encoder.as_ref().ok(),"encoder_error":encoder.err().map(|e|e.to_string())}),
            )
        }
        Commands::Schema { command } => {
            let mut root = Cli::command();
            root.build();
            let selected = if let Some(name) = command {
                root.find_subcommand(name)
                    .ok_or_else(|| anyhow::anyhow!("Unknown command: {name}"))?
            } else {
                &root
            };
            Ok(
                json!({"name":selected.get_name(),"about":selected.get_about().map(|s|s.to_string()),
                "commands":selected.get_subcommands().map(|c|json!({"name":c.get_name(),"about":c.get_about().map(|s|s.to_string())})).collect::<Vec<_>>(),
                "arguments":selected.get_arguments().map(|a|json!({"name":a.get_id().as_str(),"long":a.get_long(),"short":a.get_short(),"required":a.is_required_set(),"help":a.get_help().map(|s|s.to_string()),"action":format!("{:?}",a.get_action()),"type":format!("{:?}",a.get_value_parser().type_id()),"default":a.get_default_values().iter().map(|v|v.to_string_lossy()).collect::<Vec<_>>(),"values":a.get_value_parser().possible_values().map(|v|v.map(|p|p.get_name().to_owned()).collect::<Vec<_>>())})).collect::<Vec<_>>()}),
            )
        }
    }
}

pub fn demo() -> History {
    use crate::history::{Commit, FileState, Snapshot};
    use std::collections::BTreeMap;
    let groups = [
        (
            "src/core",
            vec![
                "engine.rs",
                "world.rs",
                "physics.rs",
                "terrain.rs",
                "growth.rs",
                "clock.rs",
                "color.rs",
                "seed.rs",
                "camera.rs",
            ],
        ),
        (
            "src/ui",
            vec![
                "App.tsx",
                "Garden.tsx",
                "Timeline.tsx",
                "Button.tsx",
                "Panel.tsx",
                "styles.css",
                "theme.scss",
                "index.html",
            ],
        ),
        (
            "tests",
            vec![
                "growth.test.ts",
                "world_test.go",
                "test_history.py",
                "render.spec.ts",
                "cli.test.ts",
            ],
        ),
        (
            "docs",
            vec![
                "README.md",
                "getting-started.md",
                "species.md",
                "guide.md",
                "changelog.md",
                "license.txt",
            ],
        ),
        (
            "assets",
            vec![
                "fern.png",
                "flower.svg",
                "lily.png",
                "water.webp",
                "wind.wav",
            ],
        ),
        (
            "scripts",
            vec![
                "grow.py",
                "release.sh",
                "check.sh",
                "build.js",
                "harvest.py",
            ],
        ),
        (
            "data",
            vec![
                "schema.sql",
                "garden.prisma",
                "query.graphql",
                "pond.csv",
                "reeds.tsv",
                "soil.R",
                "rain.jl",
            ],
        ),
        (
            ".",
            vec![
                "Cargo.toml",
                "package.json",
                "Cargo.lock",
                "Dockerfile",
                ".gitignore",
                "config.yaml",
                "LICENSE",
            ],
        ),
        (
            "web",
            vec![
                "index.vue",
                "garden.svelte",
                "home.html",
                "petals.scss",
                "lily.css",
            ],
        ),
        (
            "lib",
            vec![
                "garden.rb",
                "flower.ex",
                "world.go",
                "leaf.c",
                "tree.java",
                "apple.swift",
                "iris.php",
                "heather.hs",
                "violet.lua",
                "pine.cs",
            ],
        ),
    ];
    let paths: Vec<String> = groups
        .iter()
        .flat_map(|(dir, names)| {
            names.iter().map(move |name| {
                if *dir == "." {
                    name.to_string()
                } else {
                    format!("{dir}/{name}")
                }
            })
        })
        .collect();
    let messages = [
        "Plant the first seeds",
        "Lay out the islands",
        "A new species takes root",
        "Teach the garden to grow",
        "Add a little color",
        "Let the tests bloom",
        "Tend the old branches",
        "Make room for new life",
    ];
    let mut snapshots = vec![];
    for step in 0..48 {
        let count = ((step + 1) * paths.len() / 48).max(1);
        let mut files = BTreeMap::new();
        for (i, path) in paths.iter().take(count).enumerate() {
            if step > 34 && i == 3 {
                continue;
            }
            let born = i * 48 / paths.len();
            let changed = if step > born + 5 && (step + i) % 9 == 0 {
                step
            } else {
                born
            };
            files.insert(
                path.clone(),
                FileState {
                    oid: format!("demo{i}-{changed}"),
                    bytes: 200 + i as u64 * 283,
                    born,
                    changed,
                    touches: 1 + (step - born) as u32 / 9,
                },
            );
        }
        let timestamp = 1_735_689_600 + step as i64 * 86400 * 3;
        let date = chrono::DateTime::from_timestamp(timestamp, 0)
            .unwrap()
            .to_rfc3339();
        snapshots.push(Snapshot {
            commit: Commit {
                oid: format!("{:040x}", step + 1),
                timestamp,
                date,
                subject: messages[step % messages.len()].into(),
            },
            files,
        });
    }
    History {
        name: "gitarium".into(),
        root: PathBuf::from("bundled-demo"),
        total_commits: 48,
        shallow: false,
        snapshots,
    }
}

pub fn json_output(format: Format) -> bool {
    match format {
        Format::Json => true,
        Format::Human => false,
        Format::Auto => !std::io::stdout().is_terminal(),
    }
}
pub fn emit(result: Result<Value>, format: Format) -> i32 {
    match result {
        Ok(data) => {
            if json_output(format) {
                println!(
                    "{}",
                    json!({"ok":true,"data":data,"meta":{"schema_version":"1.0.0","version":env!("CARGO_PKG_VERSION")}})
                );
            } else if let Some(markdown) = data.get("markdown").and_then(Value::as_str) {
                println!("{markdown}");
                if let Some(url) = data.get("request_url").and_then(Value::as_str) {
                    eprintln!("Request the first render before embedding: {url}");
                }
            } else if let Some(path) = data.get("output").and_then(Value::as_str) {
                println!("Grown and saved to {path}");
            } else {
                println!("{}", serde_json::to_string_pretty(&data).unwrap());
            }
            0
        }
        Err(error) => {
            if json_output(format) {
                println!(
                    "{}",
                    json!({"ok":false,"error":{"code":"runtime_error","message":format!("{error:#}"),"retryable":false},"meta":{"schema_version":"1.0.0"}})
                );
            } else {
                eprintln!("gitarium: {error:#}");
            }
            1
        }
    }
}
