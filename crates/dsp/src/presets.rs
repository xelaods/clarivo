//! 組み込みのEQプリセット。数値は耳で調整する前の初期値。

use crate::biquad::FilterKind::*;
use crate::dynamics::LevelerSettings;
use crate::peq::{Band, EqSettings};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPreset {
    pub id: String,
    pub name: String,
    pub description: String,
    /// 組み込み（true）かユーザー作成（false）か
    #[serde(default)]
    pub builtin: bool,
    pub eq: EqSettings,
    #[serde(default)]
    pub leveler: LevelerSettings,
}

fn eq(bands: Vec<Band>) -> EqSettings {
    EqSettings {
        enabled: true,
        auto_preamp: true,
        preamp_db: 0.0,
        bands,
    }
}

pub fn builtin_presets() -> Vec<ChannelPreset> {
    vec![
        ChannelPreset {
            id: "flat".into(),
            name: "フラット".into(),
            description: "EQをかけない".into(),
            builtin: true,
            eq: EqSettings::default(),
            leveler: LevelerSettings::default(),
        },
        ChannelPreset {
            id: "footsteps".into(),
            name: "足音強調".into(),
            description: "低音を抑えて足音の帯域を持ち上げ、小さい音も聞き取りやすくする".into(),
            builtin: true,
            eq: eq(vec![
                Band::new(LowShelf, 120.0, -6.0, 0.707),
                Band::new(Peaking, 3200.0, 4.0, 1.0),
                Band::new(Peaking, 6500.0, -1.5, 2.0),
            ]),
            leveler: LevelerSettings {
                enabled: true,
                threshold_db: -30.0,
                ratio: 2.0,
                max_boost_db: 12.0,
                floor_db: -65.0,
                attack_ms: 10.0,
                release_ms: 250.0,
            },
        },
        ChannelPreset {
            id: "tame_gunshots".into(),
            name: "銃声を抑える".into(),
            description: "爆発音や銃声の耳に痛い帯域を下げる".into(),
            builtin: true,
            eq: eq(vec![
                Band::new(LowShelf, 100.0, -4.0, 0.707),
                Band::new(Peaking, 1800.0, -2.0, 1.0),
                Band::new(HighShelf, 8000.0, -3.0, 0.707),
            ]),
            leveler: LevelerSettings::default(),
        },
        ChannelPreset {
            id: "voice_clarity".into(),
            name: "声を聞き取りやすく".into(),
            description: "こもりを減らし、声の輪郭を出す（通話向け）".into(),
            builtin: true,
            eq: eq(vec![
                Band::new(HighPass, 90.0, 0.0, 0.707),
                Band::new(Peaking, 250.0, -2.0, 1.0),
                Band::new(Peaking, 2800.0, 3.0, 1.2),
                Band::new(HighShelf, 8000.0, 1.5, 0.707),
            ]),
            leveler: LevelerSettings::default(),
        },
        ChannelPreset {
            id: "bass".into(),
            name: "低音強め".into(),
            description: "迫力を出す".into(),
            builtin: true,
            eq: eq(vec![
                Band::new(LowShelf, 90.0, 5.0, 0.707),
                Band::new(Peaking, 300.0, -1.0, 1.0),
            ]),
            leveler: LevelerSettings::default(),
        },
        ChannelPreset {
            id: "movie".into(),
            name: "映画・動画".into(),
            description: "台詞を聞き取りやすくし、音量差を小さくする".into(),
            builtin: true,
            eq: eq(vec![
                Band::new(LowShelf, 80.0, 2.0, 0.707),
                Band::new(Peaking, 2500.0, 2.0, 1.0),
            ]),
            leveler: LevelerSettings {
                enabled: true,
                threshold_db: -26.0,
                ratio: 1.6,
                max_boost_db: 8.0,
                floor_db: -60.0,
                attack_ms: 15.0,
                release_ms: 400.0,
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_have_unique_ids_and_sane_boost() {
        let p = builtin_presets();
        let mut ids: Vec<_> = p.iter().map(|p| p.id.clone()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), p.len());
        for preset in &p {
            assert!(preset.eq.max_boost_db() < 8.0, "{}", preset.id);
            assert!(preset.eq.bands.len() <= crate::peq::MAX_BANDS);
        }
    }
}
