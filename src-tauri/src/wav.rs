// WAV plumbing: crash-recovery for takes whose RIFF header went stale mid-write,
// duration probing, and the sample-perfect concat used by export.
use anyhow::{anyhow, bail, Context, Result};
use std::fs;
use std::path::Path;

/// Walk RIFF chunks; return byte offset of the `data` chunk header.
fn find_data_chunk(bytes: &[u8]) -> Result<usize> {
    if bytes.len() < 44 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        bail!("not a RIFF/WAVE file");
    }
    let mut off = 12usize;
    while off + 8 <= bytes.len() {
        let id = &bytes[off..off + 4];
        let size = u32::from_le_bytes(bytes[off + 4..off + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            return Ok(off);
        }
        // chunks are word-aligned
        off += 8 + size + (size & 1);
    }
    bail!("no data chunk found");
}

/// If the file's RIFF/data size fields don't match its real length (crash mid-take),
/// patch them in place. Returns true if a patch was applied.
pub fn recover_wav(path: &Path) -> Result<bool> {
    let mut bytes = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let len = bytes.len();
    let data_off = find_data_chunk(&bytes)?;
    let real_riff = (len - 8) as u32;
    let real_data = (len - data_off - 8) as u32;
    let riff_field = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let data_field = u32::from_le_bytes(bytes[data_off + 4..data_off + 8].try_into().unwrap());
    if riff_field == real_riff && data_field == real_data {
        return Ok(false);
    }
    bytes[4..8].copy_from_slice(&real_riff.to_le_bytes());
    bytes[data_off + 4..data_off + 8].copy_from_slice(&real_data.to_le_bytes());
    fs::write(path, &bytes).with_context(|| format!("patch {}", path.display()))?;
    Ok(true)
}

pub fn wav_duration_secs(path: &Path) -> Result<f64> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    Ok(reader.duration() as f64 / spec.sample_rate as f64)
}

/// Concatenate takes (all must share one format) with `gap_ms` of silence between
/// passages. Pure sample append — provably lossless.
#[allow(dead_code)] // used by export (P4)
pub fn concat_wavs(inputs: &[std::path::PathBuf], gap_ms: u32, out: &Path) -> Result<()> {
    let first = inputs.first().ok_or_else(|| anyhow!("nothing to concat"))?;
    let spec = hound::WavReader::open(first)?.spec();
    let gap_samples = (spec.sample_rate as u64 * gap_ms as u64 / 1000) * spec.channels as u64;
    let mut writer = hound::WavWriter::create(out, spec)?;
    for (i, input) in inputs.iter().enumerate() {
        let mut reader = hound::WavReader::open(input)?;
        let s = reader.spec();
        if s != spec {
            bail!(
                "format mismatch in {} ({}Hz/{}ch/{}bit vs session {}Hz/{}ch/{}bit)",
                input.display(),
                s.sample_rate, s.channels, s.bits_per_sample,
                spec.sample_rate, spec.channels, spec.bits_per_sample
            );
        }
        for sample in reader.samples::<i32>() {
            writer.write_sample(sample?)?;
        }
        if i + 1 < inputs.len() {
            for _ in 0..gap_samples {
                writer.write_sample(0i32)?;
            }
        }
    }
    writer.finalize()?;
    Ok(())
}

/// Downsampled peak envelope for the inline crop waveform: `buckets` peaks in
/// 0..1, one streaming pass. Mono 24-bit, so sample index == frame index.
pub fn waveform_peaks(path: &Path, buckets: usize) -> Result<Vec<f32>> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let total = reader.len() as usize; // total samples (mono → frames)
    if buckets == 0 || total == 0 {
        return Ok(vec![0.0; buckets]);
    }
    let per = total.div_ceil(buckets); // samples per bucket
    let max_amp = (1i64 << (spec.bits_per_sample - 1)) as f32; // 24-bit → 2^23
    let mut out = vec![0f32; buckets];
    for (i, s) in reader.into_samples::<i32>().enumerate() {
        let v = (s? as f32).abs() / max_amp;
        let b = (i / per).min(buckets - 1);
        if v > out[b] {
            out[b] = v;
        }
    }
    Ok(out)
}

/// Like `concat_wavs` but each input keeps only its non-destructive cut spans —
/// `spans[i]` is the ordered, non-overlapping list of (start, end) seconds to
/// KEEP for `inputs[i]` (empty = keep the whole file). Interior gaps between an
/// input's spans are dropped seamlessly (mid-segment cuts); the inter-passage
/// `gap_ms` silence is inserted only BETWEEN inputs. Spans are in seconds so
/// they survive upstream resampling — sample offsets are recomputed per file.
pub fn concat_wavs_segments(
    inputs: &[std::path::PathBuf],
    spans: &[Vec<(f64, f64)>],
    gap_ms: u32,
    out: &Path,
) -> Result<()> {
    let first = inputs.first().ok_or_else(|| anyhow!("nothing to concat"))?;
    let spec = hound::WavReader::open(first)?.spec();
    let gap_samples = (spec.sample_rate as u64 * gap_ms as u64 / 1000) * spec.channels as u64;
    let mut writer = hound::WavWriter::create(out, spec)?;
    for (i, input) in inputs.iter().enumerate() {
        let mut reader = hound::WavReader::open(input)?;
        let s = reader.spec();
        if s != spec {
            bail!(
                "format mismatch in {} ({}Hz/{}ch/{}bit vs session {}Hz/{}ch/{}bit)",
                input.display(),
                s.sample_rate, s.channels, s.bits_per_sample,
                spec.sample_rate, spec.channels, spec.bits_per_sample
            );
        }
        // mono: frame index == sample index. No kept spans → whole file.
        let rate = s.sample_rate as f64;
        let ranges: Vec<(u64, u64)> = match spans.get(i) {
            Some(v) if !v.is_empty() => v
                .iter()
                .map(|(a, b)| ((a * rate).round() as u64, (b * rate).round() as u64))
                .collect(),
            _ => vec![(0, u64::MAX)],
        };
        let mut ri = 0usize; // current kept range (ranges are sorted, disjoint)
        for (j, sample) in reader.samples::<i32>().enumerate() {
            let j = j as u64;
            while ri < ranges.len() && j >= ranges[ri].1 {
                ri += 1;
            }
            if ri >= ranges.len() {
                break; // past the last kept span — stop reading
            }
            if j >= ranges[ri].0 {
                writer.write_sample(sample?)?;
            } else {
                sample?; // inside a cut — consume + propagate read errors, skip
            }
        }
        if i + 1 < inputs.len() {
            for _ in 0..gap_samples {
                writer.write_sample(0i32)?;
            }
        }
    }
    writer.finalize()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_wav(path: &Path, n_samples: u32) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 48_000,
            bits_per_sample: 24,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(path, spec).unwrap();
        for i in 0..n_samples {
            w.write_sample(((i % 1000) as i32 - 500) * 1000).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn recover_patches_stale_header() {
        let dir = std::env::temp_dir().join("booth-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("stale.wav");
        make_wav(&p, 48_000); // 1.0 s

        // simulate a crash: append raw samples the header doesn't know about
        let mut bytes = std::fs::read(&p).unwrap();
        bytes.extend(std::iter::repeat(0u8).take(48_000 * 3)); // +1.0 s of 24-bit mono
        std::fs::write(&p, &bytes).unwrap();

        assert!(recover_wav(&p).unwrap(), "should patch");
        assert!(!recover_wav(&p).unwrap(), "second pass is a no-op");
        let dur = wav_duration_secs(&p).unwrap();
        assert!((dur - 2.0).abs() < 0.01, "duration {dur} should be ~2.0");
    }

    #[test]
    fn concat_is_sample_exact() {
        let dir = std::env::temp_dir().join("booth-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.wav");
        let b = dir.join("b.wav");
        let out = dir.join("out.wav");
        make_wav(&a, 48_000); // 1 s
        make_wav(&b, 24_000); // 0.5 s
        concat_wavs(&[PathBuf::from(&a), PathBuf::from(&b)], 350, &out).unwrap();
        let dur = wav_duration_secs(&out).unwrap();
        assert!((dur - 1.85).abs() < 0.001, "1s + 0.35s gap + 0.5s = 1.85, got {dur}");
    }

    #[test]
    fn segmented_concat_keeps_only_kept_spans() {
        let dir = std::env::temp_dir().join("booth-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("seg.wav");
        let out = dir.join("seg-out.wav");
        make_wav(&a, 48_000); // 1.0 s @ 48k

        // edge trims: keep [0.25, 0.75] → 0.5 s
        concat_wavs_segments(&[PathBuf::from(&a)], &[vec![(0.25, 0.75)]], 0, &out).unwrap();
        assert!((wav_duration_secs(&out).unwrap() - 0.5).abs() < 0.001);

        // interior cut: keep [0,0.25] + [0.75,1.0] (middle removed) → 0.5 s
        concat_wavs_segments(&[PathBuf::from(&a)], &[vec![(0.0, 0.25), (0.75, 1.0)]], 0, &out)
            .unwrap();
        assert!((wav_duration_secs(&out).unwrap() - 0.5).abs() < 0.001);

        // no spans → lossless full duration
        concat_wavs_segments(&[PathBuf::from(&a)], &[vec![]], 0, &out).unwrap();
        assert!((wav_duration_secs(&out).unwrap() - 1.0).abs() < 0.001);
    }

    #[test]
    fn waveform_peaks_shape() {
        let dir = std::env::temp_dir().join("booth-wav-test");
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("peaks.wav");
        make_wav(&a, 48_000);
        let peaks = waveform_peaks(&a, 100).unwrap();
        assert_eq!(peaks.len(), 100);
        assert!(peaks.iter().all(|p| *p >= 0.0 && *p <= 1.0), "peaks in 0..1");
        assert!(peaks.iter().any(|p| *p > 0.0), "non-silent input has signal");
    }
}
