//! チャンネルのミックスと ChatMix。

/// ChatMix の値（-1.0 = ゲームのみ, 0 = 等量, +1.0 = チャットのみ）から
/// (ゲームの倍率, チャットの倍率) を求める。
///
/// SteelSeries の ChatMix と同じく、中央では両方とも 1.0、片側に寄せると
/// 反対側だけが下がっていく（中央で全体の音量が下がらないようにするため）。
pub fn chatmix_gains(mix: f32) -> (f32, f32) {
    let m = mix.clamp(-1.0, 1.0);
    if m >= 0.0 {
        (1.0 - m, 1.0)
    } else {
        (1.0, 1.0 + m)
    }
}

/// 7.1ch（FL FR FC LFE BL BR SL SR）を 2ch に混ぜる（ITU 方式に近い係数）。
pub fn downmix_71_to_stereo(input: &[&[f32]; 8], left: &mut [f32], right: &mut [f32]) {
    const C: f32 = 0.707;
    const S: f32 = 0.707;
    const LFE: f32 = 0.5;
    for i in 0..left.len() {
        let fl = input[0][i];
        let fr = input[1][i];
        let fc = input[2][i] * C;
        let lfe = input[3][i] * LFE;
        let bl = input[4][i] * S;
        let br = input[5][i] * S;
        let sl = input[6][i] * S;
        let sr = input[7][i] * S;
        // 2ch の音源（FL/FR 以外が 0）は音量を変えずに通す。
        // 7.1ch 全部が鳴ると大きくなるが、最後のリミッターで抑える
        left[i] = fl + fc + lfe + bl + sl;
        right[i] = fr + fc + lfe + br + sr;
    }
}

/// `dst += src * gain`
#[inline]
pub fn mix_into(dst: &mut [f32], src: &[f32], gain: f32) {
    if gain == 0.0 {
        return;
    }
    for (d, s) in dst.iter_mut().zip(src) {
        *d += *s * gain;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chatmix_center_keeps_both() {
        assert_eq!(chatmix_gains(0.0), (1.0, 1.0));
        assert_eq!(chatmix_gains(1.0), (0.0, 1.0));
        assert_eq!(chatmix_gains(-1.0), (1.0, 0.0));
        let (g, c) = chatmix_gains(0.5);
        assert!((g - 0.5).abs() < 1e-6 && c == 1.0);
    }
}
