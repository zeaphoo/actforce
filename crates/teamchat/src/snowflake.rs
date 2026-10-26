use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const DISCORD_EPOCH: u64 = 1_420_070_400_000;

static INCREMENT: AtomicU64 = AtomicU64::new(0);
static LAST_MS: AtomicU64 = AtomicU64::new(0);

pub fn generate() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    let ms = now - DISCORD_EPOCH;

    let last = LAST_MS.swap(ms, Ordering::SeqCst);
    let inc = if ms == last {
        INCREMENT.fetch_add(1, Ordering::SeqCst) & 0xFFF
    } else {
        INCREMENT.store(0, Ordering::SeqCst);
        0
    };

    let snowflake: u64 = (ms << 22) | inc;
    snowflake.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generates_snowflake() {
        let id = generate();
        let n: u64 = id.parse().unwrap();
        assert!(n > 0);
    }

    #[test]
    fn test_unique_sequence() {
        let mut ids = Vec::new();
        for _ in 0..100 {
            ids.push(generate());
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 100);
    }
}
