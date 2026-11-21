//! Shared HTTP client (rustls + the OS trust store) and a resumable-safe file downloader.

use std::{path::Path, sync::OnceLock, time::Duration};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

pub const USER_AGENT: &str = concat!("cerolingo-bot-manager/", env!("CARGO_PKG_VERSION"));

pub fn client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(Duration::from_secs(20))
            .read_timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build the HTTP client")
    })
}

fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

async fn download_once(url: &str, dest: &Path, on_progress: &mut (dyn FnMut(f64) + Send)) -> Result<String> {
    let mut res = client().get(url).send().await?;
    if !res.status().is_success() {
        bail!("download failed (HTTP {})", res.status().as_u16());
    }
    let total = res.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest)
        .await
        .with_context(|| format!("cannot write {}", dest.display()))?;
    let mut hash = Sha256::new();
    let mut received = 0u64;
    while let Some(chunk) = res.chunk().await? {
        hash.update(&chunk);
        file.write_all(&chunk).await?;
        received += chunk.len() as u64;
        if total > 0 {
            on_progress(received as f64 / total as f64);
        }
    }
    file.flush().await?;
    file.sync_all().await?;
    if total > 0 && received != total {
        bail!("download incomplete ({received} of {total} bytes)");
    }
    Ok(to_hex(&hash.finalize()))
}

/// Download `url` into `dest` (3 attempts). Returns the SHA-256 of the file as lowercase hex.
/// `dest` is removed when every attempt fails.
pub async fn download(url: &str, dest: &Path, mut on_progress: impl FnMut(f64) + Send) -> Result<String> {
    let mut last = None;
    for attempt in 0..3 {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_secs(2 * attempt)).await;
        }
        match download_once(url, dest, &mut on_progress).await {
            Ok(sha) => return Ok(sha),
            Err(err) => last = Some(err),
        }
    }
    let _ = tokio::fs::remove_file(dest).await;
    Err(last.expect("at least one attempt"))
}

pub async fn get_json(url: &str) -> Result<serde_json::Value> {
    let res = client()
        .get(url)
        .header("Accept", "application/vnd.github+json, application/json")
        .timeout(Duration::from_secs(30))
        .send()
        .await?;
    if !res.status().is_success() {
        bail!("HTTP {}", res.status().as_u16());
    }
    Ok(res.json().await?)
}
