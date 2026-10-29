//! AutoEq（Equalizer APO 形式）のパラメトリックEQを読み込む。
//!
//! 例:
//! ```text
//! Preamp: -6.4 dB
//! Filter 1: ON LSC Fc 105 Hz Gain 5.6 dB Q 0.70
//! Filter 2: ON PK Fc 2150 Hz Gain -2.8 dB Q 1.41
//! ```

use crate::biquad::FilterKind;
use crate::peq::{Band, EqSettings, MAX_BANDS};

#[derive(Debug, PartialEq)]
pub struct ParseError(pub String);

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ParseError {}

/// テキストを読み込み、ヘッドホン補正用の `EqSettings` を作る。
/// AutoEq はプリアンプ込みで設計されているので、自動プリアンプは使わない。
pub fn parse_equalizer_apo(text: &str) -> Result<EqSettings, ParseError> {
    let mut preamp = 0.0f32;
    let mut bands = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("preamp:") {
            preamp = first_number(rest)
                .ok_or_else(|| ParseError(format!("プリアンプの値が読めません: {line}")))?;
            continue;
        }
        if !lower.starts_with("filter") {
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let upper: Vec<String> = tokens.iter().map(|t| t.to_ascii_uppercase()).collect();
        if !upper.iter().any(|t| t == "ON") {
            continue;
        }
        let kind = upper.iter().find_map(|t| match t.as_str() {
            "PK" | "PEQ" => Some(FilterKind::Peaking),
            "LS" | "LSC" => Some(FilterKind::LowShelf),
            "HS" | "HSC" => Some(FilterKind::HighShelf),
            "LP" | "LPQ" => Some(FilterKind::LowPass),
            "HP" | "HPQ" => Some(FilterKind::HighPass),
            _ => None,
        });
        let Some(kind) = kind else { continue };
        let value_after = |key: &str| -> Option<f32> {
            upper
                .iter()
                .position(|t| t == key)
                .and_then(|i| tokens.get(i + 1))
                .and_then(|v| v.replace(',', ".").parse::<f32>().ok())
        };
        let freq = value_after("FC")
            .ok_or_else(|| ParseError(format!("周波数が読めません: {line}")))?;
        let gain = value_after("GAIN").unwrap_or(0.0);
        let q = value_after("Q").unwrap_or(0.707);
        bands.push(Band::new(kind, freq, gain, q));
    }
    if bands.is_empty() {
        return Err(ParseError("フィルターが見つかりません".into()));
    }
    if bands.len() > MAX_BANDS {
        bands.truncate(MAX_BANDS);
    }
    Ok(EqSettings {
        enabled: true,
        auto_preamp: false,
        preamp_db: preamp,
        bands,
    })
}

fn first_number(s: &str) -> Option<f32> {
    s.split_whitespace()
        .find_map(|t| t.replace(',', ".").parse::<f32>().ok())
}

/// AutoEq リポジトリの INDEX.md から（機種名, 結果フォルダへの相対パス）を取り出す。
///
/// 行の形式: `- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990`
pub fn parse_autoeq_index(md: &str) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for line in md.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("- [") else {
            continue;
        };
        let Some(name_end) = rest.find("](") else {
            continue;
        };
        let name = &rest[..name_end];
        let after = &rest[name_end + 2..];
        // 機種名に括弧を含むことがあるので「) by」を区切りにする
        let Some(path_end) = after.find(") by ").or_else(|| after.rfind(')')) else {
            continue;
        };
        let path = after[..path_end].trim_start_matches("./");
        let source = after[path_end + 1..]
            .trim()
            .strip_prefix("by ")
            .unwrap_or("")
            .trim()
            .to_string();
        out.push((name.to_string(), path.to_string(), source));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "Preamp: -6.4 dB\n\
        Filter 1: ON LSC Fc 105 Hz Gain 5.6 dB Q 0.70\n\
        Filter 2: ON PK Fc 2150 Hz Gain -2.8 dB Q 1.41\n\
        Filter 3: OFF PK Fc 3000 Hz Gain 9 dB Q 1\n\
        Filter 4: ON HSC Fc 10000 Hz Gain -3.1 dB Q 0.70\n";

    #[test]
    fn parses_autoeq_parametric() {
        let eq = parse_equalizer_apo(SAMPLE).unwrap();
        assert_eq!(eq.preamp_db, -6.4);
        assert_eq!(eq.bands.len(), 3);
        assert_eq!(eq.bands[0].kind, FilterKind::LowShelf);
        assert_eq!(eq.bands[1].freq, 2150.0);
        assert_eq!(eq.bands[1].q, 1.41);
        assert_eq!(eq.bands[2].gain_db, -3.1);
        assert!(!eq.auto_preamp);
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_equalizer_apo("Preamp: -1 dB").is_err());
    }

    #[test]
    fn parses_index_lines() {
        let md = "# Index\n- [Sennheiser HD 650](./oratory1990/over-ear/Sennheiser%20HD%20650) by oratory1990\n- [AKG K371](./crinacle/GRAS%2043AG-7%20over-ear/AKG%20K371) by crinacle on GRAS 43AG-7\n";
        let idx = parse_autoeq_index(md);
        assert_eq!(idx.len(), 2);
        let tricky = parse_autoeq_index(
            "- [1MORE Aero (ANC Off)](./HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)) by HypetheSonics on GRAS RA0045",
        );
        assert_eq!(
            tricky[0].1,
            "HypetheSonics/GRAS%20RA0045%20in-ear/1MORE%20Aero%20(ANC%20Off)"
        );
        assert_eq!(idx[0].0, "Sennheiser HD 650");
        assert_eq!(idx[0].1, "oratory1990/over-ear/Sennheiser%20HD%20650");
        assert_eq!(idx[0].2, "oratory1990");
    }
}
