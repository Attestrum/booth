// Export: pure-Rust sample-perfect concat of kept takes -> narration/voice.wav,
// then ffmpeg -> voice.mp3 (44.1 kHz mono — the align.sh / sync-to-vo.py
// contract). Existing outputs are backed up to booth/replaced/ first.
use crate::session::{self, Session};
use crate::wav;
use anyhow::{anyhow, bail, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};

const GAP_MS: u32 = 0; // no gap between passages — beats abut directly (founder)

fn emit(app: &AppHandle, msg: &str) {
    let _ = app.emit("export:progress", msg.to_string());
}

fn ffmpeg_bin() -> PathBuf {
    let brew = Path::new("/opt/homebrew/bin/ffmpeg");
    if brew.exists() {
        return brew.to_path_buf();
    }
    // dock-launched apps don't inherit the shell PATH; try `which` via login shell
    if let Ok(out) = Command::new("/bin/zsh").args(["-lc", "which ffmpeg"]).output() {
        let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    PathBuf::from("ffmpeg")
}

/// Locate a runnable ffmpeg, if any. ffmpeg is OPTIONAL: WAV export is pure
/// Rust and always works; mp3 encode and mixed-rate resampling light up when
/// ffmpeg is installed.
pub fn ffmpeg_available() -> Option<PathBuf> {
    let bin = ffmpeg_bin();
    Command::new(&bin)
        .arg("-version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| bin)
}

/// The concat target: the highest sample rate among the takes (the better
/// takes lose nothing; the lower-rate ones get upsampled).
fn pick_target(specs: &[hound::WavSpec]) -> hound::WavSpec {
    *specs
        .iter()
        .max_by_key(|s| s.sample_rate)
        .expect("at least one take")
}

/// Mixed sample rates within a session are allowed (rate gate removed,
/// founder 2026-06-12): when takes disagree, resample the minority-rate ones
/// to the target via ffmpeg into a temp dir. Uniform sessions pass through
/// untouched and keep the sample-perfect pure-Rust concat path.
fn normalize_rates(inputs: &[PathBuf], progress: &dyn Fn(&str)) -> Result<Vec<PathBuf>> {
    let specs: Vec<hound::WavSpec> = inputs
        .iter()
        .map(|p| Ok(hound::WavReader::open(p)?.spec()))
        .collect::<Result<_>>()?;
    let target = pick_target(&specs);
    if specs.iter().all(|s| *s == target) {
        return Ok(inputs.to_vec());
    }
    let Some(ffmpeg) = ffmpeg_available() else {
        let mut rates: Vec<u32> = specs.iter().map(|s| s.sample_rate).collect();
        rates.sort_unstable();
        rates.dedup();
        bail!(
            "takes were recorded at mixed sample rates ({} Hz) — resampling needs \
             ffmpeg (brew install ffmpeg), or re-record the minority takes on one device",
            rates
                .iter()
                .map(|r| r.to_string())
                .collect::<Vec<_>>()
                .join(" / ")
        );
    };
    let codec = match target.bits_per_sample {
        16 => "pcm_s16le",
        32 => "pcm_s32le",
        _ => "pcm_s24le",
    };
    let tmp = std::env::temp_dir().join(format!("booth-export-{}", std::process::id()));
    fs::create_dir_all(&tmp)?;
    let n = specs.iter().filter(|s| **s != target).count();
    progress(&format!("RESAMPLE ▸ {n} TAKES → {} Hz", target.sample_rate));
    let mut out = Vec::with_capacity(inputs.len());
    for (i, (input, spec)) in inputs.iter().zip(&specs).enumerate() {
        if *spec == target {
            out.push(input.clone());
            continue;
        }
        let dest = tmp.join(format!("rs{i:03}.wav"));
        let status = Command::new(&ffmpeg)
            .args([
                "-y",
                "-v",
                "error",
                "-i",
                input.to_str().unwrap(),
                "-ar",
                &target.sample_rate.to_string(),
                "-ac",
                &target.channels.to_string(),
                "-c:a",
                codec,
                dest.to_str().unwrap(),
            ])
            .status()
            .map_err(|e| anyhow!("ffmpeg launch failed: {e}"))?;
        if !status.success() {
            bail!("resample of {} failed ({status})", input.display());
        }
        out.push(dest);
    }
    Ok(out)
}

/// Filesystem-safe stem from the document/episode name: keep spaces, replace
/// path separators / control chars, never empty.
fn export_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' => '-',
            c if c.is_control() => '-',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "voice".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A stem under `dir` for which neither `<stem>.wav` nor `<stem>.mp3` exists —
/// appends " (1)", " (2)", … so a re-export NEVER overwrites a prior one
/// (gap #27; supersedes the timestamped-backup scheme of gap #3).
fn dedup_base(dir: &Path, stem: &str) -> String {
    let taken =
        |s: &str| dir.join(format!("{s}.wav")).exists() || dir.join(format!("{s}.mp3")).exists();
    if !taken(stem) {
        return stem.to_string();
    }
    let mut n = 1;
    loop {
        let cand = format!("{stem} ({n})");
        if !taken(&cand) {
            return cand;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_names_after_document_and_dedups() {
        // stem sanitation: path separators out, spaces kept, never empty
        assert_eq!(export_stem("My Script"), "My Script");
        assert_eq!(export_stem("a/b:c"), "a-b-c");
        assert_eq!(export_stem("   "), "voice");

        let dir = std::env::temp_dir().join("booth-export-test/dedup");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // first export claims the bare name
        assert_eq!(dedup_base(&dir, "doc"), "doc");
        fs::write(dir.join("doc.wav"), "x").unwrap();
        // a clashing wav (or mp3) bumps to " (1)", then " (2)"
        assert_eq!(dedup_base(&dir, "doc"), "doc (1)");
        fs::write(dir.join("doc (1).mp3"), "x").unwrap();
        assert_eq!(dedup_base(&dir, "doc"), "doc (2)");
    }

    fn make_wav(path: &Path, rate: u32, secs: f64) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: rate,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..(rate as f64 * secs) as u32 {
            w.write_sample(((i % 1000) as i32 - 500) * 1000).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn normalize_passes_uniform_through_and_resamples_mixed() {
        let dir = std::env::temp_dir().join("booth-export-test/normalize");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a48.wav");
        let b = dir.join("b48.wav");
        let c = dir.join("c24.wav");
        make_wav(&a, 48_000, 0.2);
        make_wav(&b, 48_000, 0.2);
        make_wav(&c, 24_000, 0.2);

        // uniform: identical paths back, no temp files
        let same = normalize_rates(&[a.clone(), b.clone()], &|_| {}).unwrap();
        assert_eq!(same, vec![a.clone(), b.clone()]);

        // mixed: the 24k take is resampled to 48k. ffmpeg is an optional
        // runtime dependency — without it this half of the test would only
        // see the clear mixed-rate error, so skip it (CI installs ffmpeg).
        if ffmpeg_available().is_none() {
            eprintln!("ffmpeg not installed; skipping the resample half");
            assert!(normalize_rates(&[a.clone(), c.clone()], &|_| {}).is_err());
            return;
        }
        let mixed = normalize_rates(&[a.clone(), c.clone()], &|_| {}).unwrap();
        assert_eq!(mixed[0], a, "matching take untouched");
        assert_ne!(mixed[1], c, "minority take replaced by resampled temp");
        let spec = hound::WavReader::open(&mixed[1]).unwrap().spec();
        assert_eq!(spec.sample_rate, 48_000);
        assert_eq!(spec.channels, 1);
        // and the normalized set now concats cleanly
        let out = dir.join("out.wav");
        wav::concat_wavs(&mixed, 350, &out).unwrap();
        let dur = wav::wav_duration_secs(&out).unwrap();
        assert!((dur - 0.75).abs() < 0.01, "0.2 + 0.35 + 0.2 ≈ 0.75, got {dur}");
    }
}

pub fn export(
    app: &AppHandle,
    episode_dir: &Path,
    session: &Session,
    allow_partial: bool,
) -> Result<(PathBuf, Option<PathBuf>)> {
    let missing = session
        .passages
        .iter()
        .filter(|p| p.takes.is_empty())
        .count();
    if missing > 0 && !allow_partial {
        bail!("{missing} passages have no take — record them or export partial");
    }

    let takes_dir = session::takes_dir(episode_dir);
    // selected take per passage, with its kept (non-cut) spans — kept aligned so
    // spans[i] belongs to inputs[i] (normalize_rates preserves order).
    let mut inputs: Vec<PathBuf> = Vec::new();
    let mut spans: Vec<Vec<(f64, f64)>> = Vec::new();
    for p in &session.passages {
        if let Some(t) = p.selected_take() {
            inputs.push(takes_dir.join(&t.file));
            spans.push(t.kept_spans());
        }
    }
    if inputs.is_empty() {
        bail!("nothing recorded yet");
    }

    // export lands NEXT TO the document, named after it; a re-export never
    // clobbers — it gets a " (N)" suffix (founder 2026-06-12, gap #27).
    let base = dedup_base(episode_dir, &export_stem(&session.episode));

    let inputs = normalize_rates(&inputs, &|m| emit(app, m))?;
    emit(app, &format!("CONCAT ▸ {} TAKES", inputs.len()));
    let wav_out = episode_dir.join(format!("{base}.wav"));
    wav::concat_wavs_segments(&inputs, &spans, GAP_MS, &wav_out)?;

    let mp3_out = if let Some(ffmpeg) = ffmpeg_available() {
        emit(app, &format!("ENCODE ▸ {base}.mp3"));
        let mp3 = episode_dir.join(format!("{base}.mp3"));
        let status = Command::new(ffmpeg)
            .args([
                "-y",
                "-v",
                "error",
                "-i",
                wav_out.to_str().unwrap(),
                "-ar",
                "44100",
                "-ac",
                "1",
                "-b:a",
                "192k",
                mp3.to_str().unwrap(),
            ])
            .status()
            .map_err(|e| anyhow!("ffmpeg launch failed: {e}"))?;
        if !status.success() {
            bail!("ffmpeg exited with {status}");
        }
        Some(mp3)
    } else {
        emit(app, &format!("NO FFMPEG ▸ {base}.wav only (brew install ffmpeg for mp3)"));
        None
    };

    emit(app, "SEALED");
    Ok((wav_out, mp3_out))
}
