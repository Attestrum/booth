// Export: pure-Rust sample-perfect concat of kept takes -> narration/voice.wav,
// then ffmpeg -> voice.mp3 (44.1 kHz mono — the align.sh / sync-to-vo.py
// contract). Existing outputs are backed up to booth/replaced/ first.
use crate::session::{self, Session};
use crate::wav;
use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Emitter};

const GAP_MS: u32 = 350; // breath room between passages

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
        let status = Command::new(ffmpeg_bin())
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

/// Timestamped backup names so a second export can NEVER clobber the first
/// backup (DESIGN.md gap fix #3 — the original could be an ElevenLabs track).
fn backup(narration: &Path, episode_dir: &Path, name: &str) -> Result<()> {
    let target = narration.join(name);
    if target.exists() {
        let dir = session::booth_dir(episode_dir).join("replaced");
        fs::create_dir_all(&dir)?;
        let mut ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut dest = dir.join(format!("{name}.{ts}.bak"));
        while dest.exists() {
            ts += 1;
            dest = dir.join(format!("{name}.{ts}.bak"));
        }
        fs::rename(&target, &dest).with_context(|| format!("backup {name}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backups_never_clobber() {
        let ep = std::env::temp_dir().join("booth-export-test/ep");
        let _ = fs::remove_dir_all(&ep);
        let narration = ep.join("narration");
        fs::create_dir_all(&narration).unwrap();

        for content in ["first", "second"] {
            fs::write(narration.join("voice.mp3"), content).unwrap();
            backup(&narration, &ep, "voice.mp3").unwrap();
        }
        let replaced = session::booth_dir(&ep).join("replaced");
        let baks: Vec<_> = fs::read_dir(&replaced).unwrap().flatten().collect();
        assert_eq!(baks.len(), 2, "both backups must survive");
        backup(&narration, &ep, "voice.mp3").unwrap(); // no target — no-op
        assert_eq!(fs::read_dir(&replaced).unwrap().count(), 2);
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

        // mixed: the 24k take is resampled to 48k (needs ffmpeg — local-only
        // test box, same dependency the mp3 encode step already requires)
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
) -> Result<(PathBuf, PathBuf)> {
    let missing = session
        .passages
        .iter()
        .filter(|p| p.takes.is_empty())
        .count();
    if missing > 0 && !allow_partial {
        bail!("{missing} passages have no take — record them or export partial");
    }

    let takes_dir = session::takes_dir(episode_dir);
    let inputs: Vec<PathBuf> = session
        .passages
        .iter()
        .filter_map(|p| p.takes.last())
        .map(|t| takes_dir.join(&t.file))
        .collect();
    if inputs.is_empty() {
        bail!("nothing recorded yet");
    }

    let narration = episode_dir.join("narration");
    fs::create_dir_all(&narration)?;
    backup(&narration, episode_dir, "voice.wav")?;
    backup(&narration, episode_dir, "voice.mp3")?;

    let inputs = normalize_rates(&inputs, &|m| emit(app, m))?;
    emit(app, &format!("CONCAT ▸ {} TAKES", inputs.len()));
    let wav_out = narration.join("voice.wav");
    wav::concat_wavs(&inputs, GAP_MS, &wav_out)?;

    emit(app, "ENCODE ▸ voice.mp3");
    let mp3_out = narration.join("voice.mp3");
    let status = Command::new(ffmpeg_bin())
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
            mp3_out.to_str().unwrap(),
        ])
        .status()
        .map_err(|e| anyhow!("ffmpeg launch failed: {e}"))?;
    if !status.success() {
        bail!("ffmpeg exited with {status}");
    }

    emit(app, "SEALED");
    Ok((wav_out, mp3_out))
}
