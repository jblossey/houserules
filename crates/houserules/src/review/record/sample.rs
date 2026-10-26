//! The post-merge sample draw of a change record (spec 8.5).
//!
//! A record whose route is `auto-merge-eligible` is sampled for a post-merge
//! audit when a number drawn on its key is below `review.route.sampleRate`.
//! The draw reads the key and nothing else, so a re-run on the same key gives
//! the same answer, and no author can steer it before the merge.

use crate::baseline;

/// The characters of the hash that the draw reads: the first eight bytes.
const DRAW_DIGITS: usize = 16;

/// 2^64, the range of the draw.
const DRAW_RANGE: f64 = 18_446_744_073_709_551_616.0;

/// Whether the record keyed by `key` is sampled at `rate` (0 to 1). The draw is
/// the first eight bytes of the SHA-256 of `houserules-sample:<key>`, read as a
/// big-endian integer and divided by 2^64; the record is sampled when the draw
/// is below `rate`. A rate of 1 samples every key, and a rate of 0 none.
pub(crate) fn drawn(key: &str, rate: f64) -> bool {
    if rate >= 1.0 {
        return true;
    }
    let digest = baseline::hash(format!("houserules-sample:{key}").as_bytes());
    let value = u64::from_str_radix(&digest[..DRAW_DIGITS], 16)
        .expect("the digits of a hash are hex digits");
    (value as f64 / DRAW_RANGE) < rate
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A key whose draw is 0.8819: the first eight bytes of the SHA-256 of
    /// `houserules-sample:` and this key, read as a big-endian integer, are
    /// `0xe1c59b1e5227242a` (`sha256sum`).
    const HIGH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    /// A key whose draw is 0.3503 (`0x59ae5752f78be140`).
    const LOW: &str = "0123456789abcdef0123456789abcdef01234567";

    /// A key whose draw is 0.4925 (`0x7e16cfac61e2c4a6`).
    const MIDDLE: &str = "ffffffffffffffffffffffffffffffffffffffff";

    #[test]
    fn a_key_is_sampled_when_its_draw_is_below_the_rate() {
        assert!(!drawn(HIGH, 0.88));
        assert!(drawn(HIGH, 0.89));
        assert!(!drawn(LOW, 0.34));
        assert!(drawn(LOW, 0.36));
        assert!(!drawn(MIDDLE, 0.49));
        assert!(drawn(MIDDLE, 0.50));
    }

    #[test]
    fn a_rate_of_zero_samples_no_key_and_a_rate_of_one_samples_every_key() {
        for key in [HIGH, LOW, MIDDLE] {
            assert!(!drawn(key, 0.0), "{key}");
            assert!(drawn(key, 1.0), "{key}");
        }
    }

    #[test]
    fn the_same_key_gives_the_same_answer_every_time() {
        for _ in 0..3 {
            assert!(drawn(LOW, 0.36));
            assert!(!drawn(LOW, 0.34));
        }
    }

    #[test]
    fn the_draw_reads_the_key_and_nothing_else() {
        assert_ne!(
            drawn(HIGH, 0.5),
            drawn(LOW, 0.5),
            "two keys with draws on both sides of the rate"
        );
    }
}
