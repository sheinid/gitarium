use crate::{history::History, render::Renderer};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Debug, Serialize)]
pub struct VideoOptions {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub duration: f32,
    pub hold: f32,
    pub calendar: bool,
    pub transition: f32,
}
impl VideoOptions {
    pub fn validate(&self) -> Result<()> {
        if self.width < 320
            || self.width > 3840
            || self.height < 180
            || self.height > 2160
            || self.width % 2 != 0
            || self.height % 2 != 0
        {
            bail!("Video dimensions must be even, with width 320..3840 and height 180..2160");
        }
        if self.fps == 0 || self.fps > 60 {
            bail!("--fps must be 1..60");
        }
        if !self.duration.is_finite() || !(1.0..=600.0).contains(&self.duration) {
            bail!("--duration must be 1..600 seconds");
        }
        if !self.hold.is_finite() || self.hold < 0.0 || self.hold >= self.duration {
            bail!("--hold must be nonnegative and shorter than --duration");
        }
        if !self.transition.is_finite() || !(0.1..=5.0).contains(&self.transition) {
            bail!("--transition must be 0.1..5 seconds");
        }
        Ok(())
    }
}

pub fn ffmpeg(requested: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = requested {
        return check_encoder(path);
    }
    if let Some(path) = std::env::var_os("GITARIUM_FFMPEG") {
        return check_encoder(Path::new(&path));
    }
    let mut candidates = vec![];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            }));
        }
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tools/ffmpeg"));
    candidates.push(PathBuf::from("ffmpeg"));
    for path in candidates {
        if check_encoder(&path).is_ok() {
            return Ok(path);
        }
    }
    bail!(
        "A working FFmpeg with libx264 is required for MP4 export. Install FFmpeg, or set GITARIUM_FFMPEG to its executable path. PNG snapshots work without it."
    )
}
fn check_encoder(path: &Path) -> Result<PathBuf> {
    let out = Command::new(path)
        .args(["-hide_banner", "-encoders"])
        .output()
        .with_context(|| format!("Cannot run FFmpeg at {}", path.display()))?;
    if !out.status.success() || !String::from_utf8_lossy(&out.stdout).contains("libx264") {
        bail!(
            "FFmpeg at {} cannot encode H.264 with libx264",
            path.display()
        );
    }
    Ok(path.into())
}
pub fn output_parent(path: &Path, force: bool) -> Result<PathBuf> {
    if path.exists() && !force {
        bail!(
            "Output already exists: {}. Pass --force to replace it.",
            path.display()
        );
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("Cannot create output directory {}", parent.display()))?;
    Ok(parent.into())
}
pub fn save_png(image: &image::RgbaImage, path: &Path, force: bool) -> Result<()> {
    let parent = output_parent(path, force)?;
    let temp = tempfile::Builder::new()
        .prefix(".gitarium-")
        .suffix(".png")
        .tempfile_in(parent)?;
    image.save_with_format(temp.path(), image::ImageFormat::Png)?;
    if force {
        temp.persist(path)?;
    } else {
        temp.persist_noclobber(path)?;
    }
    Ok(())
}

/// Git clocks may run backwards. Calendar playback uses a monotone envelope.
pub fn calendar_position(history: &History, progress: f32) -> f32 {
    let mut times = Vec::new();
    let mut latest = i64::MIN;
    for s in &history.snapshots {
        latest = latest.max(s.commit.timestamp);
        times.push(latest);
    }
    let first = times[0];
    let last = *times.last().unwrap();
    if first == last {
        return progress * (times.len() - 1) as f32;
    }
    let target = first as f64 + (last - first) as f64 * progress as f64;
    let index = times
        .partition_point(|&t| t as f64 <= target)
        .saturating_sub(1);
    if index >= times.len() - 1 {
        return (times.len() - 1) as f32;
    }
    let span = (times[index + 1] - times[index]).max(1) as f64;
    index as f32 + ((target - times[index] as f64) / span) as f32
}

pub fn export(
    history: &History,
    renderer: &mut Renderer,
    options: &VideoOptions,
    path: &Path,
    encoder: &Path,
    force: bool,
) -> Result<usize> {
    options.validate()?;
    let parent = output_parent(path, force)?;
    let temp = tempfile::Builder::new()
        .prefix(".gitarium-")
        .suffix(".mp4")
        .tempfile_in(&parent)?;
    let log = tempfile::tempfile()?;
    let mut command = Command::new(encoder);
    command
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pixel_format",
            "rgba",
            "-video_size",
        ])
        .arg(format!("{}x{}", options.width, options.height))
        .arg("-framerate")
        .arg(options.fps.to_string())
        .args([
            "-i",
            "pipe:0",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "19",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
        ])
        .arg(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(log.try_clone()?));
    let mut child = command.spawn().context("Could not start FFmpeg")?;
    let mut stdin = child.stdin.take().context("FFmpeg stdin is unavailable")?;
    let frames = (options.duration * options.fps as f32).round() as usize;
    let transition = options
        .transition
        .min((options.duration - options.hold) * 0.5);
    let active = ((options.duration - options.hold - transition) * options.fps as f32)
        .round()
        .max(2.0) as usize;
    let result = (|| -> Result<()> {
        for frame in 0..frames {
            let progress = (frame as f32 / (active - 1) as f32).min(1.0);
            let position = if options.calendar {
                calendar_position(history, progress)
            } else {
                progress * (history.snapshots.len() - 1) as f32
            };
            let image = renderer.animated_frame(
                history,
                position,
                frame as f32 / options.fps as f32,
                options.width,
                options.height,
                transition,
            );
            stdin
                .write_all(image.as_raw())
                .context("FFmpeg stopped accepting video frames")?;
            if frame % options.fps as usize == 0 || frame + 1 == frames {
                eprintln!(
                    "{}",
                    serde_json::json!({"event":"render_progress","frame":frame+1,"frames":frames,"percent":((frame+1)*100/frames)})
                );
            }
        }
        Ok(())
    })();
    drop(stdin);
    if result.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    if !status.success() || result.is_err() {
        use std::io::{Seek, SeekFrom};
        let mut reader: File = log;
        reader.seek(SeekFrom::Start(0))?;
        let mut error = String::new();
        reader.take(16384).read_to_string(&mut error)?;
        bail!(
            "Video encoding failed: {}{}",
            result
                .err()
                .map(|e| e.to_string() + ". ")
                .unwrap_or_default(),
            error.trim()
        );
    }
    if force {
        temp.persist(path)?;
    } else {
        temp.persist_noclobber(path)?;
    }
    Ok(frames)
}
