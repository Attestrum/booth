//! yt-dlp sidecar: probe a URL for an existing caption track and, failing that,
//! download its audio to a transient file. Invoked with `std::process::Command`
//! on the bundled `externalBin` binary (same pattern `export.rs` uses for
//! ffmpeg) — no extra async runtime. The caller is responsible for deleting any
//! downloaded media (the engine does this on every exit).

use anyhow::{bail, Context, Result};
use serde_json::Map;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptionKind {
    Manual,
    Auto,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptionChoice {
    pub lang: String,
    pub kind: CaptionKind,
}

/// A located caption file plus how it should be tagged.
#[derive(Clone, Debug)]
pub struct FetchedCaption {
    pub file: PathBuf,
    pub kind: CaptionKind,
    pub lang: String,
}

/// Result of one caption-fetch extraction: metadata always; a caption file when
/// a preferred-language track existed.
#[derive(Clone, Debug)]
pub struct Fetched {
    pub title: String,
    pub duration_sec: f64,
    pub caption: Option<FetchedCaption>,
}

/// Resolve the yt-dlp binary: bundled next to the executable (release), the dev
/// `binaries/` sidecar, or `yt-dlp` on PATH as a last resort.
pub fn resolve_bin() -> PathBuf {
    // Tauri strips the target triple from externalBin sidecars but keeps the
    // platform's executable extension, so the bundled binary is `yt-dlp.exe` on
    // Windows and `yt-dlp` elsewhere.
    let bundled_name = if cfg!(windows) { "yt-dlp.exe" } else { "yt-dlp" };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join(bundled_name);
            if p.exists() {
                return p;
            }
        }
    }
    // Dev sidecar: fetch-yt-dlp.sh names it by full target triple.
    let dev_name = if cfg!(windows) {
        "yt-dlp-x86_64-pc-windows-msvc.exe"
    } else {
        "yt-dlp-aarch64-apple-darwin"
    };
    let dev = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(dev_name);
    if dev.exists() {
        return dev;
    }
    PathBuf::from(bundled_name)
}

/// A configured yt-dlp invoker. Holds the binary path and, optionally, a
/// browser to pull cookies from. Cookies are OFF by default: reading Chrome's
/// cookie store is slow (it decrypts thousands of entries) and, because the
/// sidecar is a distinct binary, triggers a macOS Keychain authorization prompt
/// that can hang the download behind the app window. The bundled yt-dlp fetches
/// YouTube, TikTok, Instagram, etc. fine anonymously — it solves YouTube's
/// challenge itself (falling back to a player client that needs no JS engine
/// when deno/node are absent, as they are in the sandboxed app). `with_cookies`
/// remains an escape hatch if YouTube ever tightens its wall again.
pub struct YtDlp {
    bin: PathBuf,
    cookies_browser: Option<String>,
}

impl YtDlp {
    /// Default invoker: resolved binary, no cookies (see the struct docs for why
    /// — slow keychain access + a hanging auth prompt). Use [`with_cookies`] to
    /// opt a YouTube request into Chrome cookies.
    pub fn new() -> Self {
        Self {
            bin: resolve_bin(),
            cookies_browser: None,
        }
    }

    /// Override the cookie source browser (or `None` to send no cookies). Pass
    /// `Some("chrome")` for a YouTube URL that hits the anonymous bot wall.
    #[allow(dead_code)]
    pub fn with_cookies(mut self, browser: Option<String>) -> Self {
        self.cookies_browser = browser;
        self
    }

    fn cmd(&self) -> Command {
        let mut c = Command::new(&self.bin);
        c.arg("--no-warnings");
        if let Some(b) = &self.cookies_browser {
            c.args(["--cookies-from-browser", b]);
        }
        c
    }

    /// `yt-dlp --version`, or None if the binary can't be run (UI surfaces this
    /// like `ffmpeg_status`). Does not need cookies.
    pub fn version(&self) -> Option<String> {
        let out = Command::new(&self.bin).arg("--version").output().ok()?;
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    /// One extraction: write the best preferred-language caption track (if any)
    /// AND a metadata sidecar (`--write-info-json`), returning title, duration,
    /// and the located caption file. This replaces the old probe-then-download
    /// pair — YouTube extraction is ~25s, so doing it once (not twice) halves
    /// the caption path. `out_dir` must be a scratch dir the caller cleans up.
    pub fn fetch_captions(&self, url: &str, out_dir: &Path, prefer: &[&str]) -> Result<Fetched> {
        let langs = prefer.join(",");
        let out = self
            .cmd()
            .args([
                "--skip-download",
                "--write-subs",
                "--write-auto-subs",
                "--write-info-json",
                "--sub-langs",
                &langs,
                "--sub-format",
                "vtt/srt/best",
                "-o",
                out_dir.join("cap").to_str().context("out dir utf8")?,
                url,
            ])
            .output()
            .context("run yt-dlp caption fetch")?;
        if !out.status.success() {
            bail!("{}", clean_err(&out.stderr));
        }

        let info: serde_json::Value = std::fs::read_to_string(out_dir.join("cap.info.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));

        // The info.json's caption maps tell us whether a preferred track exists
        // and whether it's manual or auto; the written file gives us the cues.
        let caption = match pick_caption(&info, prefer) {
            Some(choice) => find_caption_file(out_dir).map(|file| FetchedCaption {
                file,
                kind: choice.kind,
                lang: choice.lang,
            }),
            None => None,
        };

        Ok(Fetched {
            title: info
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("transcript")
                .to_string(),
            duration_sec: info.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0),
            caption,
        })
    }

    /// Download the URL's audio to a transient file in `out_dir`. Prefers
    /// symphonia-decodable containers (m4a/mp3) so no ffmpeg post-processing is
    /// needed; the caller deletes it after transcription.
    ///
    /// The trailing `best[ext=mp4]/best` fallback covers sites like TikTok that
    /// expose no audio-only stream — only muxed video+audio. yt-dlp would
    /// otherwise fail with "Requested format is not available"; instead we pull
    /// the muxed mp4 and `decode.rs` picks its AAC audio track via symphonia's
    /// isomp4 demuxer (the video track is skipped — see `decode_symphonia`).
    pub fn download_audio(&self, url: &str, out_dir: &Path) -> Result<PathBuf> {
        let tmpl = out_dir.join("audio.%(ext)s");
        let out = self
            .cmd()
            .args([
                "-f",
                "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio/best[ext=mp4]/best",
                "-o",
                tmpl.to_str().context("out dir utf8")?,
                url,
            ])
            .output()
            .context("run yt-dlp audio download")?;
        if !out.status.success() {
            bail!("{}", clean_err(&out.stderr));
        }
        find_audio_file(out_dir).context("yt-dlp reported success but wrote no audio file")
    }

    /// Download audio AND capture title/duration from the same extraction
    /// (`--write-info-json`). Used for platforms we don't probe for captions
    /// (TikTok/Instagram/Facebook): a separate caption pass there is just a slow,
    /// fruitless extraction, so this gets the media and its metadata in one call.
    /// Returns `(audio_file, title, duration_sec)`. Same format ladder as
    /// `download_audio`, including the muxed-mp4 fallback those sites need.
    pub fn download_audio_with_meta(
        &self,
        url: &str,
        out_dir: &Path,
    ) -> Result<(PathBuf, String, f64)> {
        let tmpl = out_dir.join("audio.%(ext)s");
        let out = self
            .cmd()
            .args([
                "--write-info-json",
                "-f",
                "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio/best[ext=mp4]/best",
                "-o",
                tmpl.to_str().context("out dir utf8")?,
                url,
            ])
            .output()
            .context("run yt-dlp audio download")?;
        if !out.status.success() {
            bail!("{}", clean_err(&out.stderr));
        }
        let audio =
            find_audio_file(out_dir).context("yt-dlp reported success but wrote no audio file")?;
        let info: serde_json::Value = std::fs::read_to_string(out_dir.join("audio.info.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let title = info
            .get("title")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("transcript")
            .to_string();
        let duration = info.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
        Ok((audio, title, duration))
    }
}

/// Platforms where we skip the caption probe and go straight to Whisper. They
/// rarely expose a usable caption track via yt-dlp, so probing just adds a slow,
/// usually-fruitless extraction before the inevitable audio download. Matched by
/// host substring (tolerates `www.`/`m.` prefixes and share/shortlink hosts).
pub fn skips_caption_probe(url: &str) -> bool {
    let u = url.to_ascii_lowercase();
    [
        "tiktok.com",
        "instagram.com",
        "facebook.com",
        "fb.watch",
        "fb.com",
    ]
    .iter()
    .any(|d| u.contains(d))
}

impl Default for YtDlp {
    fn default() -> Self {
        Self::new()
    }
}

/// Trim yt-dlp's stderr to the most useful line for the UI.
fn clean_err(stderr: &[u8]) -> String {
    let s = String::from_utf8_lossy(stderr);
    s.lines()
        .rev()
        .find(|l| l.contains("ERROR") || l.contains("error"))
        .map(|l| l.trim().trim_start_matches("ERROR:").trim().to_string())
        .unwrap_or_else(|| s.trim().lines().last().unwrap_or("yt-dlp failed").to_string())
}

// ---- helpers ------------------------------------------------------------

fn pick_caption(meta: &serde_json::Value, prefer: &[&str]) -> Option<CaptionChoice> {
    let manual = meta.get("subtitles").and_then(|v| v.as_object());
    let auto = meta.get("automatic_captions").and_then(|v| v.as_object());
    if let Some(lang) = manual.and_then(|m| pick_lang(m, prefer)) {
        return Some(CaptionChoice {
            lang,
            kind: CaptionKind::Manual,
        });
    }
    if let Some(lang) = auto.and_then(|m| pick_lang(m, prefer)) {
        return Some(CaptionChoice {
            lang,
            kind: CaptionKind::Auto,
        });
    }
    None
}

/// First preferred language (exact or `xx-REGION` prefix) present in the map.
fn pick_lang(obj: &Map<String, serde_json::Value>, prefer: &[&str]) -> Option<String> {
    for &p in prefer {
        if let Some(k) = obj
            .keys()
            .find(|k| k.as_str() == p || k.starts_with(&format!("{p}-")))
        {
            return Some(k.clone());
        }
    }
    None
}

/// The written subtitle file (.vtt/.srt) in `dir` — skips the `.info.json`.
fn find_caption_file(dir: &Path) -> Option<PathBuf> {
    first_match(dir, |name| {
        let l = name.to_ascii_lowercase();
        l.ends_with(".vtt") || l.ends_with(".srt")
    })
}

/// The downloaded `audio.*` media file, skipping the `audio.info.json` sidecar
/// that `--write-info-json` drops alongside it.
fn find_audio_file(dir: &Path) -> Option<PathBuf> {
    first_match(dir, |name| {
        name.starts_with("audio.") && !name.ends_with(".json")
    })
}

fn first_match(dir: &Path, pred: impl Fn(&str) -> bool) -> Option<PathBuf> {
    std::fs::read_dir(dir).ok()?.flatten().find_map(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        pred(&name).then(|| e.path())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefers_manual_english_over_auto() {
        let meta = json!({
            "title": "Demo",
            "subtitles": { "en": [{}], "fr": [{}] },
            "automatic_captions": { "en": [{}], "es": [{}] }
        });
        assert_eq!(
            pick_caption(&meta, &["en"]),
            Some(CaptionChoice { lang: "en".into(), kind: CaptionKind::Manual })
        );
    }

    #[test]
    fn falls_back_to_auto_when_no_manual() {
        let meta = json!({
            "subtitles": {},
            "automatic_captions": { "en-US": [{}], "es": [{}] }
        });
        assert_eq!(
            pick_caption(&meta, &["en"]),
            Some(CaptionChoice { lang: "en-US".into(), kind: CaptionKind::Auto })
        );
    }

    #[test]
    fn none_when_preferred_language_absent() {
        let meta = json!({
            "subtitles": { "de": [{}] },
            "automatic_captions": { "ja": [{}] }
        });
        assert_eq!(pick_caption(&meta, &["en"]), None);
    }

    #[test]
    fn respects_preference_order() {
        let meta = json!({ "subtitles": { "es": [{}], "en": [{}] } });
        // prefer es first
        assert_eq!(
            pick_caption(&meta, &["es", "en"]).map(|c| c.lang),
            Some("es".to_string())
        );
    }

    #[test]
    fn skips_caption_probe_for_short_video_platforms() {
        for url in [
            "https://www.tiktok.com/@user/video/123",
            "https://vm.tiktok.com/abc/",
            "https://www.instagram.com/reel/xyz/",
            "https://instagram.com/p/abc/",
            "https://www.facebook.com/watch/?v=123",
            "https://fb.watch/abc/",
        ] {
            assert!(skips_caption_probe(url), "should skip probe for {url}");
        }
    }

    #[test]
    fn probes_captions_for_youtube_and_others() {
        for url in [
            "https://www.youtube.com/watch?v=abc",
            "https://youtu.be/abc",
            "https://vimeo.com/123",
        ] {
            assert!(!skips_caption_probe(url), "should probe captions for {url}");
        }
    }
}
