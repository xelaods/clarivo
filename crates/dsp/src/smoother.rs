//! パラメーターを滑らかに変える部品。つまみを動かしたときのプチノイズを防ぐ。

/// 目標値へ一定時間かけて直線的に近づける。
#[derive(Debug, Clone)]
pub struct LinearSmoother {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
    ramp_len: u32,
}

impl LinearSmoother {
    /// `ramp_samples` は目標に到達するまでのサンプル数。
    pub fn new(value: f32, ramp_samples: u32) -> Self {
        Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
            ramp_len: ramp_samples.max(1),
        }
    }

    pub fn set_target(&mut self, target: f32) {
        if (target - self.target).abs() < f32::EPSILON && self.remaining == 0 {
            return;
        }
        self.target = target;
        self.remaining = self.ramp_len;
        self.step = (target - self.current) / self.ramp_len as f32;
    }

    /// 即座に値を変える（初期化時用）。
    pub fn reset(&mut self, value: f32) {
        self.current = value;
        self.target = value;
        self.remaining = 0;
        self.step = 0.0;
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        if self.remaining > 0 {
            self.current += self.step;
            self.remaining -= 1;
            if self.remaining == 0 {
                self.current = self.target;
            }
        }
        self.current
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn is_settled(&self) -> bool {
        self.remaining == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_target_exactly() {
        let mut s = LinearSmoother::new(0.0, 10);
        s.set_target(1.0);
        let mut last = 0.0;
        for _ in 0..10 {
            let v = s.next();
            assert!(v >= last);
            last = v;
        }
        assert_eq!(s.current(), 1.0);
        assert!(s.is_settled());
    }
}
