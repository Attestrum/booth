//! First-run download + integrity check of the Whisper model.
//!
//! The 1.6 GB `large-v3-turbo` model is NOT bundled (it would push the .dmg
//! past GitHub's 2 GB asset cap). Instead it is fetched once into the app-data
//! dir, streamed through SHA-256, and only renamed into place after the hash
//! matches — so a partial/corrupt download is never loaded (whisper.cpp hard
//! crashes on a truncated model).

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MODEL_FILE: &str = "ggml-large-v3-turbo.bin";
pub const MODEL_URL: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin";
pub const MODEL_SHA256: &str =
    "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69";

/// Absolute path of the model once present (`<app_data>/models/<file>`).
pub fn model_path(app_data: &Path) -> PathBuf {
    app_data.join("models").join(MODEL_FILE)
}

/// Whether the model is present. We only ever rename a fully-verified temp file
/// into place, so existence implies integrity.
pub fn is_present(app_data: &Path) -> bool {
    model_path(app_data).exists()
}

/// Ensure the model exists locally, downloading + verifying if missing.
/// `on_progress` receives 0..=100 during the download. Returns the model path.
pub fn ensure_model<F: FnMut(u8)>(app_data: &Path, mut on_progress: F) -> Result<PathBuf> {
    let dest = model_path(app_data);
    if dest.exists() {
        return Ok(dest);
    }
    let dir = dest
        .parent()
        .context("model path has no parent dir")?
        .to_path_buf();
    std::fs::create_dir_all(&dir)?;
    let tmp = dir.join(format!("{MODEL_FILE}.part"));

    let resp = ureq::get(MODEL_URL)
        .call()
        .context("model download request failed")?;
    let total: u64 = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(&tmp).context("create temp model file")?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut done: u64 = 0;
    let mut last_pct = u8::MAX;

    loop {
        let n = reader.read(&mut buf).context("read model stream")?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).context("write temp model file")?;
        done += n as u64;
        if total > 0 {
            let pct = ((done * 100) / total).min(100) as u8;
            if pct != last_pct {
                on_progress(pct);
                last_pct = pct;
            }
        }
    }
    file.sync_all().ok();
    drop(file);

    let got = hex(&hasher.finalize());
    if got != MODEL_SHA256 {
        let _ = std::fs::remove_file(&tmp);
        bail!("model checksum mismatch — expected {MODEL_SHA256}, got {got}");
    }
    std::fs::rename(&tmp, &dest).context("finalize model into place")?;
    Ok(dest)
}

/// Verify an existing file against the expected SHA-256 (lowercase hex).
/// Exposed for future model re-verification / repair; covered by tests.
#[allow(dead_code)]
pub fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let got = hex(&hasher.finalize());
    if got != expected {
        bail!("checksum mismatch — expected {expected}, got {got}");
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_verifies_known_vector() {
        // SHA-256("abc")
        let p = std::env::temp_dir().join("booth_sha_abc.txt");
        std::fs::write(&p, b"abc").unwrap();
        assert!(verify_sha256(
            &p,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        )
        .is_ok());
        assert!(verify_sha256(&p, "deadbeef").is_err());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn model_path_is_under_app_data_models() {
        let p = model_path(Path::new("/tmp/appdata"));
        assert!(p.ends_with("models/ggml-large-v3-turbo.bin"));
    }

    #[test]
    fn ensure_model_short_circuits_when_present() {
        let dir = std::env::temp_dir().join("booth_model_present_test");
        let models = dir.join("models");
        std::fs::create_dir_all(&models).unwrap();
        std::fs::write(models.join(MODEL_FILE), b"stub").unwrap();
        // Must NOT attempt a network download when the file already exists.
        let got = ensure_model(&dir, |_| panic!("should not download")).unwrap();
        assert_eq!(got, model_path(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
