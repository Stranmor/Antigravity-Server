#[derive(Debug, Clone)]
pub struct AIMDController {
    pub additive_increase: f64,
    pub multiplicative_decrease: f64,
    pub min_limit: u64,
    pub max_limit: u64,
}

impl Default for AIMDController {
    fn default() -> Self {
        Self {
            additive_increase: 0.05,
            multiplicative_decrease: 0.7,
            min_limit: 10,
            max_limit: 1000,
        }
    }
}

impl AIMDController {
    pub fn reward(&self, current: u64) -> u64 {
        let new = (current as f64 * (1.0 + self.additive_increase)).ceil() as u64;
        new.min(self.max_limit)
    }

    pub fn penalize(&self, current: u64) -> u64 {
        let new = (current as f64 * self.multiplicative_decrease).floor() as u64;
        new.max(self.min_limit)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeStrategy {
    None,
    CheapProbe,
    DelayedHedge,
    ImmediateHedge,
}

impl ProbeStrategy {
    pub fn from_usage_ratio(ratio: f64) -> Self {
        match ratio {
            r if r < 0.70 => ProbeStrategy::None,
            r if r < 0.85 => ProbeStrategy::CheapProbe,
            r if r < 0.95 => ProbeStrategy::DelayedHedge,
            _ => ProbeStrategy::ImmediateHedge,
        }
    }

    pub fn needs_secondary(&self) -> bool {
        matches!(self, ProbeStrategy::DelayedHedge | ProbeStrategy::ImmediateHedge)
    }

    pub fn is_fire_and_forget(&self) -> bool {
        matches!(self, ProbeStrategy::CheapProbe)
    }
}
