use std::cell::Cell;

/// A simple linear congruential generator for pseudo-random numbers
pub struct SimpleRng {
    state: Cell<u64>,
}

impl SimpleRng {
    pub fn new() -> Self {
        // Seed with current time
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;
        Self {
            state: Cell::new(seed),
        }
    }

    /// Generate a random f64 between 0.0 and 1.0
    pub fn next_f64(&self) -> f64 {
        let state = self.state.get();
        let next = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state.set(next);
        (next >> 32) as f64 / u32::MAX as f64
    }
}
