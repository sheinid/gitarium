# Hosting the embed service

The public service is at https://gitarium-f0f902-185-16-61-74.sslip.io.

```md
![My repository growing with Gitarium](https://gitarium-f0f902-185-16-61-74.sslip.io/embed/OWNER/REPOSITORY.gif)
```

No action, installation, or token is required in the repository being embedded. The first image request starts a render. Small renders can complete in the initial request. Otherwise the endpoint returns a temporary PNG with a preparing message while the GIF finishes in the background. Refresh the README after it completes. If GitHub's image proxy retains a preparing image, append `?v=2` to the image URL to request a fresh proxy copy. Query parameters do not create separate renders.

GIFs are reused for six hours. An expired GIF stays available while the next view starts a refresh. If refresh fails, the old GIF stays available. Image responses also carry an ETag and a one-hour browser cache lifetime.

## Run with Docker

```sh
docker compose up --build -d
curl http://localhost:8787/healthz
```

Put an HTTPS reverse proxy in front of port 8787, and set `GITARIUM_PUBLIC_URL` to that HTTPS origin. The Compose file binds to localhost. The image runs as a non-root user, contains both Rust binaries and FFmpeg, and stores GIFs in `/var/cache/gitarium`. Keep that directory on a persistent volume.

## Run with Rust

```sh
cargo build --release --locked --features server --bins
GITARIUM_PUBLIC_URL=http://localhost:8787 ./target/release/gitarium-server
```

Git and FFmpeg with libx264 must be installed. The server binary expects the CLI next to itself unless `GITARIUM_BIN` points to another executable.

| Environment | Default |
| --- | --- |
| `GITARIUM_LISTEN` | `127.0.0.1:8787`; Docker uses `0.0.0.0:8080` |
| `GITARIUM_CACHE_DIR` | `work/embed-cache`; Docker uses `/var/cache/gitarium` |
| `GITARIUM_PUBLIC_URL` | Public service origin |
| `GITARIUM_BIN` | `gitarium` next to the server binary |
| `GITARIUM_FFMPEG` | Automatically detected FFmpeg |
| `GITHUB_TOKEN` | Optional, for GitHub API rate limits; only public repositories are accepted |

## HTTP endpoints

| Endpoint | Response |
| --- | --- |
| `/embed/OWNER/REPOSITORY.gif` | Cached animated GIF, or an uncached preparing/error PNG |
| `/status/OWNER/REPOSITORY` | JSON status without starting a render |
| `/healthz` | JSON health and pending-job count |
| `/` | Plain-text usage instructions |

The hosted render is 960 by 540, 16 seconds at 15 fps, with 1.2-second transitions. It samples up to 120 commits from the latest 2000 available commits, shows up to 24 islands, and uses a 450-plant budget. Local CLI exports allow larger images and longer histories.

## Resource limits

The service accepts public GitHub repositories only. It rejects repositories over 100 MB according to GitHub metadata, or snapshots exceeding 10000 filtered files. Downloads have a 60-second deadline and rendering has a 150-second deadline. It runs one render at a time and keeps at most eight pending jobs. Duplicate requests share a job.

Rendered GIFs must be at most 10 MB. The persistent cache keeps at most 128 GIFs and 512 MB, removing older images first. Failed requests are remembered for five minutes to avoid repeated failed downloads. Private repository metadata and arbitrary source URLs are never accepted from image requests.

The service clones repositories bare, disables global Git configuration and credential helpers in child processes, and never checks out files or runs repository code. Only Git metadata enters the renderer. Container memory and CPU limits are included in the Compose example.
