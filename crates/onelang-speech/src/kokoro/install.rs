//! Downloading the offline voice: the ONNX model and voice styles from
//! Hugging Face, espeak-ng from its GitHub release (an MSI unpacked without
//! installing anything).

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use specta::Type;
use tokio::io::AsyncWriteExt;

use crate::{Error, Result};

const HF_REPO: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/main";
/// Full precision: the best quality, still fast on a CPU.
const MODEL_FILE: &str = "onnx/model.onnx";
const MODEL_SIZE: u64 = 325_532_232;
const VOICE_SIZE: u64 = 522_240;
const ESPEAK_MSI: &str = "https://github.com/espeak-ng/espeak-ng/releases/download/1.52.0/espeak-ng.msi";
const ESPEAK_SIZE: u64 = 12_765_862;

#[derive(Serialize, Deserialize, Type, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InstallStage {
    Model,
    Voices,
    Phonemizer,
}

#[derive(Serialize, Deserialize, Type, Clone, Debug)]
pub struct InstallProgress {
    pub stage: InstallStage,
    /// Bytes downloaded in total, across all stages.
    pub done: f64,
    pub total: f64,
}

/// Approximate download size, bytes.
pub fn download_size(voices: usize) -> u64 {
    MODEL_SIZE + VOICE_SIZE * voices as u64 + ESPEAK_SIZE
}

struct Progress<'a> {
    done: u64,
    total: u64,
    report: &'a (dyn Fn(InstallProgress) + Send + Sync),
}

impl Progress<'_> {
    fn add(&mut self, stage: InstallStage, bytes: u64) {
        self.done += bytes;
        (self.report)(InstallProgress { stage, done: self.done as f64, total: self.total.max(self.done) as f64 });
    }
}

async fn download(
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    stage: InstallStage,
    progress: &mut Progress<'_>,
) -> Result<()> {
    let resp = http.get(url).send().await?;
    if !resp.status().is_success() {
        return Err(Error::Network(format!("{url}: HTTP {}", resp.status())));
    }
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await?;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        progress.add(stage, chunk.len() as u64);
    }
    file.flush().await?;
    drop(file);
    tokio::fs::rename(&part, dest).await?;
    Ok(())
}

pub async fn install(
    http: &reqwest::Client,
    dir: &Path,
    voices: &[&str],
    report: &(dyn Fn(InstallProgress) + Send + Sync),
) -> Result<()> {
    let voices_dir = dir.join("voices");
    tokio::fs::create_dir_all(&voices_dir).await?;
    let mut progress = Progress { done: 0, total: download_size(voices.len()), report };

    let model = dir.join("model.onnx");
    if model.is_file() {
        progress.add(InstallStage::Model, MODEL_SIZE);
    } else {
        download(http, &format!("{HF_REPO}/{MODEL_FILE}"), &model, InstallStage::Model, &mut progress).await?;
    }

    for id in voices {
        let dest = voices_dir.join(format!("{id}.bin"));
        if dest.is_file() {
            progress.add(InstallStage::Voices, VOICE_SIZE);
        } else {
            download(http, &format!("{HF_REPO}/voices/{id}.bin"), &dest, InstallStage::Voices, &mut progress).await?;
        }
    }

    let espeak = dir.join("espeak");
    if espeak.join("espeak-ng.exe").is_file() {
        progress.add(InstallStage::Phonemizer, ESPEAK_SIZE);
    } else {
        let msi = dir.join("espeak-ng.msi");
        download(http, ESPEAK_MSI, &msi, InstallStage::Phonemizer, &mut progress).await?;
        let tmp = dir.join("espeak.tmp");
        let (msi2, tmp2) = (msi.clone(), tmp.clone());
        tokio::task::spawn_blocking(move || {
            let _ = std::fs::remove_dir_all(&tmp2);
            extract_msi(&msi2, &tmp2, "espeak-ng.exe")
        })
        .await
        .map_err(|e| Error::Model(e.to_string()))??;
        let _ = std::fs::remove_dir_all(&espeak);
        std::fs::rename(&tmp, &espeak)?;
        let _ = std::fs::remove_file(&msi);
    }
    Ok(())
}

fn invalid(msg: impl Into<String>) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.into())
}

/// Long name of an MSI "short|long" name, target part of "target:source".
fn long_name(name: &str) -> &str {
    let target = name.split(':').next().unwrap_or(name);
    target.rsplit('|').next().unwrap_or(target)
}

/// Unpack the files of an MSI package that live in the folder of `anchor`
/// (and its subfolders) into `out`, keeping the folder structure.
fn extract_msi(msi_path: &Path, out: &Path, anchor: &str) -> Result<()> {
    let mut pkg = msi::open(msi_path)?;
    let str_col = |row: &msi::Row, col: &str| row[col].as_str().map(str::to_string).unwrap_or_default();

    // Directory key → (parent key, long name)
    let mut dirs: HashMap<String, (String, String)> = HashMap::new();
    for row in pkg.select_rows(msi::Select::table("Directory"))? {
        let name = long_name(&str_col(&row, "DefaultDir")).to_string();
        dirs.insert(str_col(&row, "Directory"), (str_col(&row, "Directory_Parent"), name));
    }
    let dir_path = |key: &str| -> Vec<String> {
        let mut parts = vec![];
        let mut k = key.to_string();
        for _ in 0..64 {
            let Some((parent, name)) = dirs.get(&k) else { break };
            if name != "." && name != "SourceDir" {
                parts.push(name.clone());
            }
            if parent.is_empty() || parent == &k {
                break;
            }
            k = parent.clone();
        }
        parts.reverse();
        parts
    };
    let mut components: HashMap<String, String> = HashMap::new();
    for row in pkg.select_rows(msi::Select::table("Component"))? {
        components.insert(str_col(&row, "Component"), str_col(&row, "Directory_"));
    }
    // File key (= name inside the cabinet) → folder parts + file name
    let mut files: Vec<(String, Vec<String>, String)> = vec![];
    for row in pkg.select_rows(msi::Select::table("File"))? {
        let dir = components.get(&str_col(&row, "Component_")).cloned().unwrap_or_default();
        files.push((str_col(&row, "File"), dir_path(&dir), long_name(&str_col(&row, "FileName")).to_string()));
    }
    let root = files
        .iter()
        .find(|f| f.2.eq_ignore_ascii_case(anchor))
        .map(|f| f.1.clone())
        .ok_or_else(|| invalid(format!("{anchor} not found in the package")))?;

    let cabinets: Vec<String> = pkg
        .select_rows(msi::Select::table("Media"))?
        .filter_map(|row| row["Cabinet"].as_str().and_then(|c| c.strip_prefix('#')).map(str::to_string))
        .collect();
    let mut wanted: HashMap<String, PathBuf> = HashMap::new();
    for (key, parts, name) in &files {
        if parts.starts_with(&root) {
            let mut p = out.to_path_buf();
            for part in &parts[root.len()..] {
                p.push(part);
            }
            p.push(name);
            wanted.insert(key.clone(), p);
        }
    }

    // Cabinets are unpacked with the tar.exe that ships with Windows 10+
    // (libarchive reads CAB); entries are named by their File keys.
    let raw = out.join(".cab");
    std::fs::create_dir_all(&raw)?;
    for (i, cab_name) in cabinets.iter().enumerate() {
        let mut data = vec![];
        pkg.read_stream(cab_name)?.read_to_end(&mut data)?;
        let cab_path = raw.join(format!("{i}.cab"));
        std::fs::write(&cab_path, data)?;
        let mut cmd = std::process::Command::new("tar");
        cmd.arg("-xf").arg(&cab_path).arg("-C").arg(&raw);
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut cmd, 0x0800_0000); // CREATE_NO_WINDOW
        let status = cmd.status().map_err(|e| invalid(format!("tar: {e}")))?;
        if !status.success() {
            return Err(invalid(format!("tar failed to unpack {cab_name}")).into());
        }
        let _ = std::fs::remove_file(&cab_path);
    }
    for (key, dest) in wanted {
        let src = raw.join(&key);
        if !src.is_file() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&src, &dest)?;
    }
    let _ = std::fs::remove_dir_all(&raw);
    if !out.join(anchor).is_file() {
        return Err(invalid("the package could not be unpacked").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msi_names() {
        assert_eq!(long_name("ESPEAK~1|espeak-ng-data"), "espeak-ng-data");
        assert_eq!(long_name("SourceDir"), "SourceDir");
        assert_eq!(long_name("tgt|Target:src|Source"), "Target");
    }

    /// `ESPEAK_MSI=path/to/espeak-ng.msi cargo test -p onelang-speech unpack -- --ignored`
    #[test]
    #[ignore]
    fn unpack_espeak() {
        let msi = std::env::var("ESPEAK_MSI").expect("ESPEAK_MSI");
        let out = std::env::temp_dir().join("onelang-espeak-test");
        let _ = std::fs::remove_dir_all(&out);
        extract_msi(Path::new(&msi), &out, "espeak-ng.exe").unwrap();
        assert!(out.join("espeak-ng-data").join("en_dict").is_file());
    }
}
