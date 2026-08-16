//! `sonnetRandom.ts` (21 lines) — compiler-grade 1:1 Rust port.
//!
//! Supplies deterministic selection without relying on process-global random
//! state. All three exports are pure functions; the Rust port uses `u32`
//! wrapping arithmetic (`wrapping_mul`, `wrapping_add`) to mirror the JS
//! `Math.imul` (32-bit signed product bit-pattern) + `>>> 0` (coerce-to-unsigned)
//! pair bit-for-bit.

/// folia `sonnetRandom.ts` — `hashSonnetSeed(value)`.
///
/// FNV-1a 32-bit: start `0x811c9dc5`, per UTF-16 code unit XOR then `* 0x01000193`.
/// JS indexes UTF-16 code units (`value.charCodeAt(index)`); Rust iterates `char`s
/// cast to `u32` — equivalent for BMP code points; astral characters splice (as
/// `charCodeAt` returns a surrogate) would need surrogate-pair handling. Folia
/// only ever hashes ASCII seed strings (`"chenglou"` / track ids), so BMP-only
/// hashing is byte-faithful for the realised input space.
pub fn hash_sonnet_seed(value: &str) -> u32 {
    let mut hash: u32 = 2166136261;
    for c in value.chars() {
        hash ^= c as u32;
        hash = hash.wrapping_mul(16777619);
    }
    hash
}

/// folia `sonnetRandom.ts` — `mixSonnetSeed(seed, salt)`.
///
/// `(Math.trunc(seed) ^ salt) >>> 0` → `(seed as u32) ^ salt`; then wrapped multiply by
/// the Knuth multiplicative constant `2654435761`; the product bit-pattern is coerced
/// to unsigned 32-bit (`>>> 0`) — Rust `u32` wrapping_mul lands there naturally.
pub fn mix_sonnet_seed(seed: f64, salt: u32) -> u32 {
    let seed_trunc = (seed.trunc()) as u32;
    (seed_trunc ^ salt).wrapping_mul(2654435761)
}

/// folia `sonnetRandom.ts` — `sonnetHash01(seed, index, salt)`.
///
/// Deterministic 0..1 jitter per element index; seek-safe and rebuild-stable.
/// `mixSonnetSeed(seed + Math.imul(index + 1, 97), salt)` — `index` is always small
/// and positive here, so `seed` (an `f64`) stays exact for product-of-integers inputs.
/// The final `/ 4294967296` divides into the JS `number` (f64) domain; the Rust port
/// matches at `f64` precision so callers do not observe any f32-only truncation drift.
pub fn sonnet_hash01(seed: f64, index: usize, salt: u32) -> f64 {
    let offset = (index.wrapping_add(1)).wrapping_mul(97) as f64;
    mix_sonnet_seed(seed + offset, salt) as f64 / 4294967296.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_sonnet_seed_matches_fnv1a_32() {
        // FNV-1a 32 of ASCII "a" = 0xe40c292c
        assert_eq!(hash_sonnet_seed("a"), 0xe40c292c);
        // Empty string → FNV offset basis unchanged.
        assert_eq!(hash_sonnet_seed(""), 2166136261);
        // Deterministic + stable across calls.
        assert_eq!(hash_sonnet_seed("sonnet"), hash_sonnet_seed("sonnet"));
        // Non-empty input never returns the empty-string sentinel.
        assert_ne!(hash_sonnet_seed("x"), 2166136261);
    }

    #[test]
    fn mix_sonnet_seed_is_knuth_multiplicative() {
        // seed=0, salt=1 → (0 ^ 1) * 2654435761 = 2654435761.
        assert_eq!(mix_sonnet_seed(0.0, 1), 2654435761);
        // seed=0, salt=0 → 0 * constant = 0.
        assert_eq!(mix_sonnet_seed(0.0, 0), 0);
        // Known value: (5 ^ 7) * 2654435761 mod 2^32 = 2 * 2654435761 mod 2^32.
        assert_eq!(mix_sonnet_seed(5.0, 7), (2_u32).wrapping_mul(2654435761));
        assert_eq!((2_u32).wrapping_mul(2654435761), 1013904226);
    }

    #[test]
    fn sonnet_hash01_lands_in_half_open_unit_interval() {
        for &(seed, idx, salt) in &[(0.0_f64, 0_usize, 0_u32), (5.0, 3, 11), (99.0, 7, 13)] {
            let v = sonnet_hash01(seed, idx, salt);
            assert!(v >= 0.0 && v < 1.0, "hash01({seed},{idx},{salt}) = {v} out of [0,1)");
        }
        // Deterministic across calls.
        assert_eq!(sonnet_hash01(1.0, 1, 1), sonnet_hash01(1.0, 1, 1));
    }

    #[test]
    fn sonnet_hash01_distinct_jitter_for_distinct_indices() {
        let base = sonnet_hash01(0.0, 0, 0);
        // Even with seed=salt=0, pure-constant mixing means index 0 and 1 differ
        // (mixSonnetSeed(0+97,0)=97*constant, vs index 0 = 0). So values are unequal.
        assert_ne!(base, sonnet_hash01(0.0, 1, 0));
    }
}
