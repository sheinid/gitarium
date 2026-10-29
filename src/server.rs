//! Public GitHub-to-GIF service. Source repositories are cloned bare and never executed.
use crate::{
    embed::{DEFAULT_BASE_URL, Repository},
    video,
};
use anyhow::{Context, Result, bail};
use axum::{
    Json, Router,
    body::Body,
    extract::{Path as RoutePath, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use image::{Rgba, RgbaImage};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs,
    io::Cursor,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::{Duration, Instant, UNIX_EPOCH},
};
use tokio::{
    process::Command,
    sync::{Semaphore, watch},
    time::timeout,
};

const CACHE_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const ERROR_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_PENDING: usize = 8;
const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;
const MAX_CACHE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone)]
struct Config {
    cache: PathBuf,
    binary: PathBuf,
    encoder: PathBuf,
    public_url: String,
}
#[derive(Clone, Debug)]
enum JobState {
    Running,
    Ready,
    Failed(String),
}
struct Job {
    result: watch::Sender<JobState>,
    created: Instant,
}
struct App {
    config: Config,
    client: reqwest::Client,
    jobs: Mutex<BTreeMap<String, Arc<Job>>>,
    worker: Arc<Semaphore>,
    requests: Arc<Semaphore>,
}
impl App {
    fn new(config: Config) -> Result<Arc<Self>> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("gitarium/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.url().scheme() == "https"
                    && attempt.url().host_str() == Some("api.github.com")
                    && attempt.previous().len() < 3
                {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .build()?;
        Ok(Arc::new(Self {
            config,
            client,
            jobs: Mutex::new(BTreeMap::new()),
            worker: Arc::new(Semaphore::new(1)),
            requests: Arc::new(Semaphore::new(32)),
        }))
    }
    fn queue(self: &Arc<Self>, repo: Repository) -> std::result::Result<Arc<Job>, ()> {
        let slug = repo.slug();
        let job = {
            let mut jobs = self.jobs.lock().unwrap();
            jobs.retain(|_, j| {
                matches!(*j.result.borrow(), JobState::Running) || j.created.elapsed() < ERROR_TTL
            });
            if let Some(job) = jobs.get(&slug) {
                if !matches!(*job.result.borrow(), JobState::Ready) {
                    return Ok(job.clone());
                }
            }
            if jobs
                .values()
                .filter(|j| matches!(*j.result.borrow(), JobState::Running))
                .count()
                >= MAX_PENDING
            {
                return Err(());
            }
            // Completed errors are bounded too, even when many different invalid repositories are requested.
            if jobs.len() >= 128 {
                let removable = jobs
                    .iter()
                    .filter(|(_, j)| !matches!(*j.result.borrow(), JobState::Running))
                    .min_by_key(|(_, j)| j.created)
                    .map(|(key, _)| key.clone());
                if let Some(key) = removable {
                    jobs.remove(&key);
                }
            }
            let (result, _) = watch::channel(JobState::Running);
            let job = Arc::new(Job {
                result,
                created: Instant::now(),
            });
            jobs.insert(slug.clone(), job.clone());
            job
        };
        let app = self.clone();
        let task_job = job.clone();
        tokio::spawn(async move {
            let Ok(_permit) = app.worker.clone().acquire_owned().await else {
                return;
            };
            let started = Instant::now();
            let result = render_repository(&app, &repo).await;
            let state = match result {
                Ok(()) => JobState::Ready,
                Err(error) => {
                    eprintln!(
                        "{}",
                        json!({"event":"embed_render_error","repository":slug,"error":format!("{error:#}")})
                    );
                    JobState::Failed("This repository could not be rendered. It may be private, missing, too large, or temporarily unavailable.".into())
                }
            };
            eprintln!(
                "{}",
                json!({"event":"embed_render_finished","repository":slug,"ok":matches!(state,JobState::Ready),"seconds":started.elapsed().as_secs()})
            );
            task_job.result.send_replace(state);
        });
        Ok(job)
    }
}

fn router(app: Arc<App>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/healthz", get(health))
        .route(
            "/robots.txt",
            get(|| async { "User-agent: *\nDisallow: /\n" }),
        )
        .route("/embed/{owner}/{file}", get(embed))
        .route("/status/{owner}/{repo}", get(status))
        .with_state(app)
}
async fn index(State(app): State<Arc<App>>) -> String {
    format!(
        "Gitarium\n\nPaste a public GitHub repository into your README:\n\n![My repository growing with Gitarium]({}/embed/OWNER/REPOSITORY.gif)\n\nThe first request starts a render. Cached GIFs refresh after six hours when viewed.\n\nSource: https://github.com/sheinid/gitarium\nStatus: {}/status/OWNER/REPOSITORY\n",
        app.config.public_url, app.config.public_url
    )
}
async fn health(State(app): State<Arc<App>>) -> Json<serde_json::Value> {
    let jobs = app.jobs.lock().unwrap();
    Json(
        json!({"ok":true,"version":env!("CARGO_PKG_VERSION"),"pending":jobs.values().filter(|j|matches!(*j.result.borrow(),JobState::Running)).count()}),
    )
}
fn repository(owner: &str, name: &str) -> Result<Repository> {
    Repository::parse(&format!("{owner}/{name}"))
}

struct Cached {
    bytes: Vec<u8>,
    etag: String,
    stale: bool,
}
fn cached(repo: &Repository, config: &Config) -> Option<Cached> {
    let path = repo.cache_path(&config.cache);
    let metadata = fs::metadata(&path).ok()?;
    if metadata.len() > MAX_IMAGE_BYTES {
        return None;
    }
    let modified = metadata.modified().ok()?;
    let stamp = modified.duration_since(UNIX_EPOCH).ok()?.as_nanos();
    let bytes = fs::read(path).ok()?;
    if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
        return None;
    }
    Some(Cached {
        bytes,
        etag: format!("\"{stamp:x}-{:x}\"", metadata.len()),
        stale: modified.elapsed().unwrap_or_default() > CACHE_TTL,
    })
}
fn gif_response(cached: Cached, headers: &HeaderMap) -> Response {
    let not_modified =
        headers.get("if-none-match").and_then(|h| h.to_str().ok()) == Some(cached.etag.as_str());
    Response::builder()
        .status(if not_modified {
            StatusCode::NOT_MODIFIED
        } else {
            StatusCode::OK
        })
        .header("Content-Type", "image/gif")
        .header(
            "Cache-Control",
            "public, max-age=3600, stale-while-revalidate=86400",
        )
        .header("ETag", cached.etag)
        .header("X-Content-Type-Options", "nosniff")
        .body(if not_modified {
            Body::empty()
        } else {
            Body::from(cached.bytes)
        })
        .unwrap()
}
async fn embed(
    State(app): State<Arc<App>>,
    RoutePath((owner, file)): RoutePath<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let Ok(_request) = app.requests.clone().try_acquire_owned() else {
        return image_message(
            "Gitarium",
            "The renderer is busy. Try again shortly.",
            StatusCode::TOO_MANY_REQUESTS,
        );
    };
    let Some(name) = file.strip_suffix(".gif") else {
        return image_message(
            "Gitarium",
            "Image URLs must end in .gif",
            StatusCode::BAD_REQUEST,
        );
    };
    let repo = match repository(&owner, name) {
        Ok(repo) => repo,
        Err(_) => {
            return image_message(
                "Gitarium",
                "Use a valid public OWNER/REPOSITORY.",
                StatusCode::BAD_REQUEST,
            );
        }
    };
    if let Some(hit) = cached(&repo, &app.config) {
        if hit.stale {
            let _ = app.queue(repo.clone());
        }
        return gif_response(hit, &headers);
    }
    let job = match app.queue(repo.clone()) {
        Ok(job) => job,
        Err(()) => {
            return image_message(
                &repo.slug(),
                "The render queue is full. Try again shortly.",
                StatusCode::TOO_MANY_REQUESTS,
            );
        }
    };
    let mut result = job.result.subscribe();
    if matches!(*result.borrow(), JobState::Running) {
        let _ = timeout(Duration::from_secs(7), result.changed()).await;
    }
    if let Some(hit) = cached(&repo, &app.config) {
        return gif_response(hit, &headers);
    }
    let message = match result.borrow().clone() {
        JobState::Failed(error) => error,
        _ => "Growing this repository. Refresh in a moment.".into(),
    };
    image_message(&repo.slug(), &message, StatusCode::OK)
}
async fn status(
    State(app): State<Arc<App>>,
    RoutePath((owner, name)): RoutePath<(String, String)>,
) -> Response {
    let repo = match repository(&owner, &name) {
        Ok(repo) => repo,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":"invalid_repository"})),
            )
                .into_response();
        }
    };
    if let Some(hit) = cached(&repo, &app.config) {
        return Json(json!({"repository":repo.slug(),"status":if hit.stale {"stale"}else{"ready"},"bytes":hit.bytes.len()})).into_response();
    }
    let job = app.jobs.lock().unwrap().get(&repo.slug()).cloned();
    let (state, error) = match job.map(|j| j.result.borrow().clone()) {
        Some(JobState::Running) => ("rendering", None),
        Some(JobState::Failed(error)) => ("error", Some(error)),
        _ => ("not_requested", None),
    };
    Json(json!({"repository":repo.slug(),"status":state,"error":error})).into_response()
}

fn image_message(title: &str, message: &str, status: StatusCode) -> Response {
    let mut image = RgbaImage::from_pixel(960, 540, Rgba([208, 229, 219, 255]));
    fn text(image: &mut RgbaImage, text: &str, x: u32, y: u32, scale: u32) {
        for (i, ch) in text.chars().take(108 / scale as usize).enumerate() {
            if let Some(glyph) = BASIC_FONTS.get(ch) {
                for (row, bits) in glyph.iter().enumerate() {
                    for col in 0..8 {
                        if bits & (1 << col) == 0 {
                            continue;
                        }
                        for sy in 0..scale {
                            for sx in 0..scale {
                                let px = x + (i as u32 * 8 + col) * scale + sx;
                                let py = y + row as u32 * scale + sy;
                                if px < image.width() && py < image.height() {
                                    image.put_pixel(px, py, Rgba([76, 98, 79, 255]));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    text(&mut image, "gitarium", 40, 40, 2);
    text(&mut image, title, 40, 216, 2);
    let words: Vec<_> = message.split_whitespace().collect();
    let mut line = String::new();
    let mut y = 262;
    for word in words {
        if line.len() + word.len() > 100 {
            text(&mut image, &line, 40, y, 1);
            line.clear();
            y += 18;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    text(&mut image, &line, 40, y, 1);
    let mut bytes = Cursor::new(Vec::new());
    image
        .write_to(&mut bytes, image::ImageFormat::Png)
        .expect("encode built-in status image");
    Response::builder()
        .status(status)
        .header("Content-Type", "image/png")
        .header("Cache-Control", "no-store, max-age=0")
        .header("Retry-After", "30")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes.into_inner()))
        .unwrap()
}

#[derive(Deserialize)]
struct GithubRepository {
    private: bool,
    size: u64,
    name: String,
}
async fn render_repository(app: &App, repo: &Repository) -> Result<()> {
    let mut request = app
        .client
        .get(format!("https://api.github.com/repos/{}", repo.slug()))
        .header("Accept", "application/vnd.github+json");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
    }
    let response = request
        .send()
        .await
        .context("GitHub metadata request failed")?
        .error_for_status()
        .context("Repository is unavailable")?;
    let metadata: GithubRepository = response
        .json()
        .await
        .context("Invalid GitHub repository metadata")?;
    if metadata.private {
        bail!("Only public repositories can be embedded");
    }
    if metadata.size > 100_000 {
        bail!("Repository exceeds the 100 MB hosted-render limit");
    }
    // Validate the canonical name before using it as a temporary directory component.
    let _canonical = Repository::parse(&format!("{}/{}", repo.owner, metadata.name))?;
    let work = tempfile::tempdir()?;
    let checkout = work.path().join(&metadata.name);
    let mut clone = clean_command("git");
    clone
        .args([
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.file.allow=never",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "http.lowSpeedLimit=1024",
            "-c",
            "http.lowSpeedTime=15",
            "clone",
            "--bare",
            "--single-branch",
            "--no-tags",
            "--depth=2000",
            "--",
        ])
        .arg(repo.clone_url())
        .arg(&checkout);
    let output = timeout(Duration::from_secs(60), clone.output())
        .await
        .context("Repository download timed out")??;
    if !output.status.success() {
        bail!(
            "Public repository download failed: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(600)
                .collect::<String>()
        );
    }
    let rendered = work.path().join("garden.gif");
    let mut render = clean_command(&app.config.binary);
    render
        .arg("export")
        .arg(&checkout)
        .arg("-o")
        .arg(&rendered)
        .args([
            "--width",
            "960",
            "--height",
            "540",
            "--duration",
            "16",
            "--fps",
            "15",
            "--hold",
            "2",
            "--transition",
            "1.2",
            "--max-commits",
            "120",
            "--max-islands",
            "24",
            "--max-plants",
            "450",
            "--max-files",
            "10000",
        ])
        .env("GITARIUM_FFMPEG", &app.config.encoder);
    let output = timeout(Duration::from_secs(150), render.output())
        .await
        .context("GIF rendering timed out")??;
    if !output.status.success() {
        bail!(
            "GIF rendering failed: {}",
            String::from_utf8_lossy(&output.stdout)
                .chars()
                .take(800)
                .collect::<String>()
        );
    }
    let size = fs::metadata(&rendered)?.len();
    if size == 0 || size > MAX_IMAGE_BYTES {
        bail!("Rendered GIF exceeds the hosted image limit");
    }
    let destination = repo.cache_path(&app.config.cache);
    let parent = destination.parent().unwrap();
    fs::create_dir_all(parent)?;
    let temp = tempfile::NamedTempFile::new_in(parent)?;
    fs::copy(&rendered, temp.path())?;
    temp.persist(&destination)?;
    prune_cache(&app.config.cache, &destination);
    Ok(())
}
fn clean_command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_LFS_SKIP_SMUDGE", "1")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    command
}
fn prune_cache(root: &Path, keep: &Path) {
    let mut files = Vec::new();
    let Ok(owners) = fs::read_dir(root) else {
        return;
    };
    for owner in owners.flatten() {
        let Ok(entries) = fs::read_dir(owner.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            if entry.path().extension().is_some_and(|s| s == "gif") {
                if let Ok(meta) = entry.metadata() {
                    files.push((
                        entry.path(),
                        meta.len(),
                        meta.modified().unwrap_or(UNIX_EPOCH),
                    ));
                }
            }
        }
    }
    let mut bytes: u64 = files.iter().map(|f| f.1).sum();
    let mut count = files.len();
    files.sort_by_key(|f| f.2);
    for (path, size, _) in files {
        if count <= 128 && bytes <= MAX_CACHE_BYTES {
            break;
        }
        if path != keep && fs::remove_file(&path).is_ok() {
            count -= 1;
            bytes -= size;
        }
    }
}

pub async fn run() -> Result<()> {
    let address: SocketAddr = std::env::var("GITARIUM_LISTEN")
        .unwrap_or_else(|_| "127.0.0.1:8787".into())
        .parse()
        .context("Invalid GITARIUM_LISTEN address")?;
    if std::env::args().any(|a| a == "--healthcheck") {
        let health = format!("http://127.0.0.1:{}/healthz", address.port());
        reqwest::Client::new()
            .get(health)
            .timeout(Duration::from_secs(3))
            .send()
            .await?
            .error_for_status()?;
        return Ok(());
    }
    let cache = PathBuf::from(
        std::env::var_os("GITARIUM_CACHE_DIR").unwrap_or_else(|| "work/embed-cache".into()),
    );
    fs::create_dir_all(&cache)?;
    let binary = std::env::var_os("GITARIUM_BIN")
        .map(PathBuf::from)
        .unwrap_or(std::env::current_exe()?.with_file_name(if cfg!(windows) {
            "gitarium.exe"
        } else {
            "gitarium"
        }));
    if !binary.is_file() {
        bail!(
            "Build the CLI alongside the server with cargo build --release --features server --bins"
        );
    }
    let encoder = video::ffmpeg(None)?;
    let public_url = std::env::var("GITARIUM_PUBLIC_URL")
        .unwrap_or_else(|_| DEFAULT_BASE_URL.into())
        .trim_end_matches('/')
        .to_owned();
    Repository::parse("sheinid/gitarium")?.markdown(&public_url)?;
    let app = App::new(Config {
        cache,
        binary,
        encoder,
        public_url,
    })?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!(
        "{}",
        json!({"event":"embed_server_started","address":listener.local_addr()?.to_string()})
    );
    axum::serve(listener, router(app))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}
async fn shutdown() {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("listen for SIGTERM");
        tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cached_embed_health_and_validation_work_without_github() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::parse("owner/repo").unwrap();
        let path = repo.cache_path(dir.path());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let gif = b"GIF89a-cached-fixture";
        fs::write(path, gif).unwrap();
        let app = App::new(Config {
            cache: dir.path().into(),
            binary: "unused".into(),
            encoder: "unused".into(),
            public_url: "https://garden.example".into(),
        })
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move { axum::serve(listener, router(app)).await.unwrap() });
        let client = reqwest::Client::new();
        let response = client
            .get(format!("http://{address}/embed/owner/repo.gif"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["content-type"], "image/gif");
        let etag = response.headers()["etag"].clone();
        assert_eq!(&response.bytes().await.unwrap()[..], gif);
        let response = client
            .get(format!("http://{address}/embed/owner/repo.gif"))
            .header("if-none-match", etag)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
        let response = client
            .get(format!("http://{address}/healthz"))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.json::<serde_json::Value>().await.unwrap()["ok"],
            true
        );
        let response = client
            .get(format!("http://{address}/embed/bad%21/repo.gif"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response.headers()["cache-control"], "no-store, max-age=0");
        server.abort();
    }
}
