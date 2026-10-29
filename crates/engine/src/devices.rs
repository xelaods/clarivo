//! 音声デバイスの一覧と、VB-CABLE の自動検出。

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub is_default_communications: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceList {
    /// 録音デバイス（マイク、仮想ケーブルの Output 側）
    pub inputs: Vec<DeviceInfo>,
    /// 再生デバイス（ヘッドホン、仮想ケーブルの Input 側）
    pub outputs: Vec<DeviceInfo>,
}

/// VB-CABLE のどの線か。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cable {
    /// 無印（仮想マイク用）
    Main,
    /// A（Game 用）
    A,
    /// B（Chat 用）
    B,
    C,
    D,
}

/// デバイス名から VB-CABLE の線を判定する。
///
/// 名前の例: 「CABLE Input (VB-Audio Virtual Cable)」「CABLE-A Output (VB-Audio Cable A)」
pub fn cable_of(name: &str) -> Option<Cable> {
    let n = name.to_ascii_uppercase();
    if !n.contains("VB-AUDIO") && !n.contains("CABLE") {
        return None;
    }
    for (letter, cable) in [('A', Cable::A), ('B', Cable::B), ('C', Cable::C), ('D', Cable::D)] {
        if n.contains(&format!("CABLE-{letter}")) || n.contains(&format!("VB-AUDIO CABLE {letter}")) {
            return Some(cable);
        }
    }
    if n.contains("CABLE INPUT") || n.contains("CABLE OUTPUT") || n.contains("VB-AUDIO VIRTUAL CABLE") {
        return Some(Cable::Main);
    }
    None
}

/// 仮想デバイス（ヘッドホンとして選んではいけないもの）か。
pub fn is_virtual(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    cable_of(name).is_some()
        || n.contains("VB-AUDIO")
        || n.contains("VOICEMEETER")
        || n.contains("STEELSERIES SONAR")
        || n.contains("VIRTUAL AUDIO")
}

/// 一覧から、指定の線の Input（再生側）または Output（録音側）を探す。
pub fn find_cable<'a>(devices: &'a [DeviceInfo], cable: Cable) -> Option<&'a DeviceInfo> {
    devices.iter().find(|d| cable_of(&d.name) == Some(cable))
}

/// ヘッドホンを決める：指定があればそれ、なければ仮想でない既定デバイス、
/// それも無ければ仮想でない最初のデバイス。
pub fn pick_headphones<'a>(
    outputs: &'a [DeviceInfo],
    preferred: Option<&str>,
) -> Option<&'a DeviceInfo> {
    if let Some(id) = preferred {
        if let Some(d) = outputs.iter().find(|d| d.id == id) {
            return Some(d);
        }
    }
    outputs
        .iter()
        .find(|d| d.is_default && !is_virtual(&d.name))
        .or_else(|| outputs.iter().find(|d| !is_virtual(&d.name)))
}

/// マイクを決める：指定があればそれ、なければ仮想でない既定の通信用、既定、最初の順。
pub fn pick_microphone<'a>(
    inputs: &'a [DeviceInfo],
    preferred: Option<&str>,
) -> Option<&'a DeviceInfo> {
    if let Some(id) = preferred {
        if let Some(d) = inputs.iter().find(|d| d.id == id) {
            return Some(d);
        }
    }
    inputs
        .iter()
        .find(|d| d.is_default_communications && !is_virtual(&d.name))
        .or_else(|| inputs.iter().find(|d| d.is_default && !is_virtual(&d.name)))
        .or_else(|| inputs.iter().find(|d| !is_virtual(&d.name)))
}

/// 指定 ID があればそれを、無ければ VB-CABLE の該当する線を返す。
pub fn pick_cable<'a>(
    devices: &'a [DeviceInfo],
    preferred: Option<&str>,
    cable: Cable,
) -> Option<&'a DeviceInfo> {
    if let Some(id) = preferred {
        if let Some(d) = devices.iter().find(|d| d.id == id) {
            return Some(d);
        }
    }
    find_cable(devices, cable)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, name: &str, def: bool) -> DeviceInfo {
        DeviceInfo {
            id: id.into(),
            name: name.into(),
            is_default: def,
            is_default_communications: false,
        }
    }

    #[test]
    fn detects_vb_cables() {
        assert_eq!(cable_of("CABLE Input (VB-Audio Virtual Cable)"), Some(Cable::Main));
        assert_eq!(cable_of("CABLE Output (VB-Audio Virtual Cable)"), Some(Cable::Main));
        assert_eq!(cable_of("CABLE-A Input (VB-Audio Cable A)"), Some(Cable::A));
        assert_eq!(cable_of("CABLE-B Output (VB-Audio Cable B)"), Some(Cable::B));
        assert_eq!(cable_of("スピーカー (Realtek(R) Audio)"), None);
        assert_eq!(cable_of("ヘッドホン (WH-1000XM5)"), None);
    }

    #[test]
    fn headphones_skip_virtual_default() {
        let outs = vec![
            dev("a", "CABLE-A Input (VB-Audio Cable A)", true),
            dev("h", "ヘッドホン (USB Audio)", false),
        ];
        assert_eq!(pick_headphones(&outs, None).unwrap().id, "h");
        assert_eq!(pick_headphones(&outs, Some("a")).unwrap().id, "a");
    }
}
