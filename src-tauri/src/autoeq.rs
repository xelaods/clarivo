//! AutoEq（ヘッドホン補正データ、MIT）を GitHub から検索・取得する。

use clarivo_dsp::autoeq::{parse_autoeq_index, parse_equalizer_apo};
use clarivo_dsp::peq::EqSettings;
use serde::Serialize;

const BASE: &str = "https://raw.githubusercontent.com/jaakkopasanen/AutoEq/master/results";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoEqEntry {
    pub name: String,
    pub path: String,
    pub source: String,
}

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(concat!("Clarivo/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default()
}

pub async fn fetch_index(cache_file: &std::path::Path) -> Result<Vec<AutoEqEntry>, String> {
    // 1 日以内のキャッシュがあれば使う
    if let Ok(meta) = std::fs::metadata(cache_file) {
        let fresh = meta
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .map(|e| e.as_secs() < 86_400)
            .unwrap_or(false);
        if fresh {
            if let Ok(text) = std::fs::read_to_string(cache_file) {
                return Ok(to_entries(&text));
            }
        }
    }
    let text = client()
        .get(format!("{BASE}/INDEX.md"))
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("AutoEq の一覧を取得できません: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(dir) = cache_file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(cache_file, &text);
    Ok(to_entries(&text))
}

fn to_entries(md: &str) -> Vec<AutoEqEntry> {
    parse_autoeq_index(md)
        .into_iter()
        .map(|(name, path, source)| AutoEqEntry { name, path, source })
        .collect()
}

pub fn search(index: &[AutoEqEntry], query: &str) -> Vec<AutoEqEntry> {
    let words: Vec<String> = query
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .collect();
    if words.is_empty() {
        return Vec::new();
    }
    index
        .iter()
        .filter(|e| {
            let n = e.name.to_lowercase();
            words.iter().all(|w| n.contains(w))
        })
        .take(60)
        .cloned()
        .collect()
}

pub async fn load(path: &str) -> Result<EqSettings, String> {
    // 例: oratory1990/over-ear/Sennheiser%20HD%20650 → .../Sennheiser%20HD%20650%20ParametricEQ.txt
    let last = path.rsplit('/').next().unwrap_or(path);
    let url = format!("{BASE}/{path}/{last}%20ParametricEQ.txt");
    let text = client()
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| format!("補正データを取得できません: {e}"))?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    parse_equalizer_apo(&text).map_err(|e| e.to_string())
}
