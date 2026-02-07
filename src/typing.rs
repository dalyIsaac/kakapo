use crate::rng::SimpleRng;
use std::time::Duration;

// Constants for typing configuration
pub const DEFAULT_WORDS_PER_MINUTE: f64 = 80.0;
const MAX_WORDS_PER_MINUTE: f64 = 2000.0;
const CHARS_PER_WORD: f64 = 5.0; // Standard assumption: 1 word = 5 characters
const MIN_DELAY_MS: f64 = 5.0;
const MIN_SPURT_SIZE: usize = 5;
const MAX_SPURT_SIZE: usize = 15;

/// Configuration for typing variability
#[derive(Clone, Debug)]
pub struct TypingConfig {
    /// Words per minute
    pub words_per_minute: f64,
    /// Whether to enable jitter
    pub enable_jitter: bool,
}

impl Default for TypingConfig {
    fn default() -> Self {
        Self {
            words_per_minute: DEFAULT_WORDS_PER_MINUTE,
            enable_jitter: true,
        }
    }
}

impl TypingConfig {
    pub fn max_words_per_minute() -> f64 {
        MAX_WORDS_PER_MINUTE
    }
}

/// Calculate delay between keystrokes based on typing configuration
/// Implements "spurt" pattern where typing speed varies in bursts
pub fn calculate_keystroke_delay(
    config: &TypingConfig,
    char_index: usize,
    _total_chars: usize,
    rng: &SimpleRng,
) -> Duration {
    // Convert words per minute to characters per minute
    let chars_per_minute = config.words_per_minute * CHARS_PER_WORD;
    
    // Base delay from characters per minute
    let base_delay_ms = 60_000.0 / chars_per_minute;

    if !config.enable_jitter {
        return Duration::from_millis(base_delay_ms as u64);
    }

    // Create "spurts" - alternating periods of faster and slower typing
    // Each spurt is roughly 5-15 characters
    // Use deterministic spurt size based on spurt number to keep consistent spurt lengths
    let spurt_number = char_index / MIN_SPURT_SIZE;
    let spurt_size = MIN_SPURT_SIZE + ((spurt_number as f64 * 0.618033988749895) % (MAX_SPURT_SIZE - MIN_SPURT_SIZE) as f64) as usize;
    
    // Alternate between fast and slow spurts
    let spurt_multiplier = if (char_index / spurt_size) % 2 == 0 {
        0.7 + rng.next_f64() * 0.3 // Fast spurt: 70-100% of base speed
    } else {
        1.2 + rng.next_f64() * 0.6 // Slow spurt: 120-180% of base speed
    };

    // Add random jitter within the current spurt
    let jitter = 0.8 + rng.next_f64() * 0.4; // 80-120% variation

    // Apply both spurt pattern and jitter
    let final_delay_ms = base_delay_ms * spurt_multiplier * jitter;
    
    // Ensure minimum delay for reliability
    let final_delay_ms = final_delay_ms.max(MIN_DELAY_MS);

    Duration::from_millis(final_delay_ms as u64)
}
