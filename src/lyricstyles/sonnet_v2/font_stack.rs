//! Folia sonnet v2 — `utils/fontStacks.ts` font-weight normalization.
//!
//! Byte-identical 1:1 port of `normalizeFontWeight` from folia
//! `src/utils/fontStacks.ts` (119 lines) — only the function consumed by the
//! typography roles layer is ported here; the full `resolveFontStack`/` Theme.font`
//! plumbing belongs to Phase 9 (tuning) and is out of scope.
//!
//! ## folia contract
//! ```ts
//! export const MIN_FONT_WEIGHT = 100;
//! export const MAX_FONT_WEIGHT = 900;
//! export const FONT_WEIGHT_STEP = 10;
//! export const normalizeFontWeight = (fontWeight: unknown) => {
//!     if (typeof fontWeight !== 'number' && typeof fontWeight !== 'string') return 400;
//!     const parsed = parseInt(fontWeight as string, 10);
//!     if (!Number.isFinite(parsed)) return 400;
//!     const clamped = Math.min(MAX_FONT_WEIGHT, Math.max(MIN_FONT_WEIGHT, parsed));
//!     const stepped = Math.round((clamped - MIN_FONT_WEIGHT) / FONT_WEIGHT_STEP)
//!         * FONT_WEIGHT_STEP + MIN_FONT_WEIGHT;
//!     return stepped;
//! };
//! ```
//!
//! ## Rust adaptation
//! TS accepts `number | string | null | undefined`. Rust is statically typed, so
//! the port accepts a `Option<i32>` (covers the `null`/`undefined`/number cases)
//! and the typography roles layer only ever feeds a number or `None`. `parseInt`
//! semantics for a string are reproduced in `normalize_font_weight_str` for any
//! caller that holds a CSS shorthand string; the common path uses the integer
//! overload. Rounding uses `i32` arithmetic so `Math.round` (round-half-to-+∞)
//! is reproduced with Rust's `f64::round_ties_even`-equivalent — but the input is
//! always an exact multiple of 10 after clamping, so divider-round ties do not
//! occur and `round` is byte-identical to `(x + 0.5).floor()` here.

/// folia `fontStacks.ts` — `MIN_FONT_WEIGHT`.
pub const MIN_FONT_WEIGHT: i32 = 100;
/// folia `fontStacks.ts` — `MAX_FONT_WEIGHT`.
pub const MAX_FONT_WEIGHT: i32 = 900;
/// folia `fontStacks.ts` — `FONT_WEIGHT_STEP`.
pub const FONT_WEIGHT_STEP: i32 = 10;
/// folia fallback when input is neither a finite number nor a parseable string.
pub const DEFAULT_FONT_WEIGHT: i32 = 400;

/// folia `fontStacks.ts` — `normalizeFontWeight(fontWeight)`.
///
/// Accepts the integer-or-null form used by the typography roles layer. Mirrors
/// the TS short-circuit: `null`/`undefined` → `400`; non-finite → `400`; else
/// clamp to `[100, 900]` and snap to the nearest multiple of 10.
pub fn normalize_font_weight(font_weight: Option<i32>) -> i32 {
    let Some(value) = font_weight else {
        return DEFAULT_FONT_WEIGHT;
    };
    // `parseInt` of a JS number yields the truncated integer; an `Option<i32>`
    // is already integral. `Number.isFinite(parsed)` is always true here.
    let clamped = value.clamp(MIN_FONT_WEIGHT, MAX_FONT_WEIGHT);
    // `Math.round((clamped - 100) / 10) * 10 + 100`. The dividend is always an
    // exact integer; `Math.round` of a `.5` would round to +∞, but since the
    // dividend is integral (no fractional part), `round` is identity. Rust's
    // integer division truncates toward zero which differs only for negatives —
    // impossible after clamping to `[100, 900]`. Use float division + round to
    // match the TS pipeline byte-for-byte and stay robust to a future step that
    // is not a multiple of 10.
    let stepped = (((clamped - MIN_FONT_WEIGHT) as f64 / FONT_WEIGHT_STEP as f64).round()
        as i32)
        * FONT_WEIGHT_STEP
        + MIN_FONT_WEIGHT;
    stepped
}

/// folia `fontStacks.ts` — `normalizeFontWeight` string overload.
///
/// `parseInt(s, 10)` returns `NaN` for non-numeric input; `Number.isFinite(NaN)`
/// is `false` → fallback `400`. Rust mirrors this via `str::parse::<i32>()`
/// returning `Err` for non-numeric strings, which maps to the same fallback.
pub fn normalize_font_weight_str(font_weight: &str) -> i32 {
    // `parseInt` tolerates a leading sign, whitespace, and stops at the first
    // non-digit. `str::parse::<i32>()` is stricter (rejects trailing junk) so
    // we strip a leading sign + digits manually when present to reproduce the
    // lenient `parseInt` behaviour the typography tests rely on.
    let parsed = parse_int_prefix(font_weight);
    normalize_font_weight(Some(parsed))
}

/// Reproduces JS `parseInt(s, 10)` tolerance: optional sign, then the longest
/// run of ASCII digits; anything afterwards is ignored. Returns `0` for inputs
/// that start without a digit/sign+digit (matching `parseInt` → `NaN` mapped to
/// `0` because `0` flows into the `Number.isFinite` fallback path — but here
/// `0` clamps to `100` after the step, identical to TS `parseInt("garbage")` →
/// `NaN` → `400`? No: TS `NaN` short-circuits to `400`, while `0` would clamp
/// to `100`. So unmatched input MUST yield a sentinel that normalizes to `400`.
fn parse_int_prefix(s: &str) -> i32 {
    let trimmed = s.trim_start();
    let bytes = trimmed.as_bytes();
    let mut idx = 0;
    let mut sign: i32 = 1;
    if !bytes.is_empty() && (bytes[0] == b'-' || bytes[0] == b'+') {
        if bytes[0] == b'-' {
            sign = -1;
        }
        idx = 1;
    }
    let start = idx;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx == start {
        // No digits → `parseInt` returns `NaN` → `Number.isFinite` false → 400.
        return DEFAULT_FONT_WEIGHT;
    }
    match trimmed[start..idx].parse::<i32>() {
        Ok(n) => sign * n,
        Err(_) => DEFAULT_FONT_WEIGHT,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_returns_default_400() {
        // TS: `normalizeFontWeight(undefined)` === 400.
        assert_eq!(normalize_font_weight(None), DEFAULT_FONT_WEIGHT);
        assert_eq!(normalize_font_weight_str("garbage"), DEFAULT_FONT_WEIGHT);
    }

    #[test]
    fn exact_weight_passes_through() {
        assert_eq!(normalize_font_weight(Some(700)), 700);
        assert_eq!(normalize_font_weight(Some(400)), 400);
        assert_eq!(normalize_font_weight(Some(900)), 900);
        assert_eq!(normalize_font_weight(Some(100)), 100);
    }

    #[test]
    fn clamps_to_range() {
        assert_eq!(normalize_font_weight(Some(0)), 100);
        assert_eq!(normalize_font_weight(Some(50)), 100);
        assert_eq!(normalize_font_weight(Some(1000)), 900);
        assert_eq!(normalize_font_weight(Some(1500)), 900);
    }

    #[test]
    fn snaps_to_nearest_step_of_10() {
        // 254 → clamp → 254 → (254-100)/10 = 15.4 → round = 15 → 15*10+100 = 250.
        assert_eq!(normalize_font_weight(Some(254)), 250);
        // 256 → (256-100)/10 = 15.6 → round = 16 → 260.
        assert_eq!(normalize_font_weight(Some(256)), 260);
        // 155 → (155-100)/10 = 5.5 → Math.round (half-up) = 6 → 160.
        assert_eq!(normalize_font_weight(Some(155)), 160);
    }

    #[test]
    fn string_overload_parses_prefix() {
        assert_eq!(normalize_font_weight_str("700"), 700);
        assert_eq!(normalize_font_weight_str("   600abc"), 600);
        assert_eq!(normalize_font_weight_str("-900"), 100); // clamps to MIN
        assert_eq!(normalize_font_weight_str(""), DEFAULT_FONT_WEIGHT);
    }
}
