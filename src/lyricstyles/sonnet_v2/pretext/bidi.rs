//! Folia sonnet v2 — pretext `bidi.ts` (175 lines) `compiler-grade 1:1 port`.
//!
//! Simplified bidi metadata helper for the rich `prepareWithSegments()` path.
//! Forked from pdf.js via Sebastian's text-layout. Classifies characters into
//! bidi types, computes embedding levels (W1-W7 / N1-N2 / I1-I2), and maps
//! them onto prepared segments.
//!
//! ## Indexing contract
//!
//! pretext TS keeps resolved bidi classes aligned to **UTF-16 code-unit
//! offsets** (because rich prepared segments index back into the normalized
//! string with JavaScript string offsets). For byte-identical runtime behavior
//! we replicate this exact model in Rust:
//!
//!   - `classify_char` consumes a `char` (Unicode scalar value); astral code
//!     points are entered by surrogate-widening the levels array entry twice.
//!   - `compute_bidi_levels` returns `Vec<i8>` indexed by **UTF-16 code-unit
//!     positions** (i.e. one slot per 16-bit cell). Astral chars expand into
//!     two identical slots, matching TS.
//!   - `compute_segment_levels(normalized, seg_starts)` accepts byte-offset
//!     indices (the existing `analysis.rs::starts[]` contract) and translates
//!     them to UTF-16 code-unit indices before lookup, so callers stay
//!     unchanged (byte offsets are the analysis-driver's contract).

use super::bidi_data::{BidiType, LATIN1_BIDI_TYPES, NON_LATIN1_BIDI_RANGES};

/// `classifyCodePoint(codePoint)` — pretext bidi.ts:14. Binary-search
/// `nonLatin1BidiRanges` (each tuple `(lo, hi, BidiType)` in ascending order).
/// Code points ≤ 0xFF are looked up in the Latin-1 table. Default type is `L`.
pub fn classify_code_point(code: u32) -> BidiType {
    if code <= 0x00FF {
        return LATIN1_BIDI_TYPES[code as usize];
    }
    let mut lo: usize = 0;
    let mut hi: usize = NON_LATIN1_BIDI_RANGES.len() - 1;
    while lo <= hi {
        let mid = (lo + hi) >> 1;
        let (r_lo, r_hi, r_type) = NON_LATIN1_BIDI_RANGES[mid];
        if code < r_lo { hi = mid - 1; continue; }
        if code > r_hi { lo = mid + 1; continue; }
        return r_type;
    }
    BidiType::L
}

/// Decode `&str` into (codepoint, utf16_code_unit_length) pairs so the
/// `types` array stays indexed by UTF-16 code units exactly like pretext.
fn utf16_units(s: &str) -> Vec<(char, usize)> {
    s.chars().map(|c| {
        let cu_len = if (c as u32) >= 0x10000 { 2 } else { 1 };
        (c, cu_len)
    }).collect()
}

/// `computeBidiLevels(str): Int8Array | null` — pretext bidi.ts:33.
///
/// Returns `Some(Vec<i8>)` indexed by UTF-16 code-unit positions when any
/// RTL/ArabicLetter/ArabicNumber bidi type appears in the input; `None`
/// otherwise (TS returns `null`, meaning "bidirectional neutrals only").
pub fn compute_bidi_levels(s: &str) -> Option<Vec<i8>> {
    let units = utf16_units(s);
    if units.is_empty() { return None; }

    let utf16_len: usize = units.iter().map(|(_, l)| l).sum();
    let mut types: Vec<BidiType> = vec![BidiType::L; utf16_len];
    let mut saw_bidi = false;

    let mut i = 0usize;
    for &(c, cu_len) in &units {
        let t = classify_code_point(c as u32);
        if t == BidiType::R || t == BidiType::Al || t == BidiType::An {
            saw_bidi = true;
        }
        // TS: types[i + j] = t  for j in 0..codeUnitLength
        for j in 0..cu_len {
            types[i + j] = t;
        }
        i += cu_len;
    }
    if !saw_bidi { return None; }

    // Pick paragraph base direction from first strong character.
    let mut start_level: i8 = 0;
    for k in 0..utf16_len {
        match types[k] {
            BidiType::L => { start_level = 0; break; }
            BidiType::R | BidiType::Al => { start_level = 1; break; }
            _ => {}
        }
    }
    let mut levels: Vec<i8> = vec![start_level; utf16_len];

    // W2 anchors: paragraph embedding direction is `e = startLevel & 1 ? R : L`
    // (Note: original `e` setting is unused except for N0 default below; we track
    // it as `sor`.)
    let e: BidiType = if (start_level & 1) != 0 { BidiType::R } else { BidiType::L };
    let sor = e;

    // W1: NSM takes the type of its previous strong char.
    let mut last_type: BidiType = sor;
    for k in 0..utf16_len {
        if types[k] == BidiType::Nsm {
            types[k] = last_type;
        } else {
            last_type = types[k];
        }
    }

    // W2: EN after AL becomes AN; track last R/L/AL for EN resaliance.
    last_type = sor;
    for k in 0..utf16_len {
        let t = types[k];
        if t == BidiType::En {
            types[k] = if last_type == BidiType::Al { BidiType::An } else { BidiType::En };
        } else if t == BidiType::R || t == BidiType::L || t == BidiType::Al {
            last_type = t;
        }
    }

    // W3: AL -> R.
    for k in 0..utf16_len {
        if types[k] == BidiType::Al { types[k] = BidiType::R; }
    }

    // W4: ES between EN/EN -> EN; CS between EN/AN or AN/AN -> same as neighbor.
    if utf16_len >= 3 {
        for k in 1..utf16_len - 1 {
            if types[k] == BidiType::Es
                && types[k - 1] == BidiType::En
                && types[k + 1] == BidiType::En
            {
                types[k] = BidiType::En;
            }
            if types[k] == BidiType::Cs
                && (types[k - 1] == BidiType::En || types[k - 1] == BidiType::An)
                && types[k + 1] == types[k - 1]
            {
                types[k] = types[k - 1];
            }
        }
    }

    // W5: ET adjacent to EN becomes EN (outwards spread).
    for k in 0..utf16_len {
        if types[k] != BidiType::En { continue; }
        // back
        let mut j = k as isize - 1;
        while j >= 0 && types[j as usize] == BidiType::Et {
            types[j as usize] = BidiType::En;
            j -= 1;
        }
        // forward
        let mut j = k + 1;
        while j < utf16_len && types[j] == BidiType::Et {
            types[j] = BidiType::En;
            j += 1;
        }
    }

    // W6: WS/ES/ET/CS -> ON.
    for k in 0..utf16_len {
        let t = types[k];
        if t == BidiType::Ws || t == BidiType::Es || t == BidiType::Et || t == BidiType::Cs {
            types[k] = BidiType::On;
        }
    }

    // W7: EN surrounded by L on the left becomes L.
    last_type = sor;
    for k in 0..utf16_len {
        let t = types[k];
        if t == BidiType::En {
            types[k] = if last_type == BidiType::L { BidiType::L } else { BidiType::En };
        } else if t == BidiType::R || t == BidiType::L {
            last_type = t;
        }
    }

    // N1: run of ON between two strong types with same direction inherits that direction.
    let mut k = 0;
    while k < utf16_len {
        if types[k] != BidiType::On { k += 1; continue; }
        let start = k;
        let mut end = k + 1;
        while end < utf16_len && types[end] == BidiType::On { end += 1; }
        let before: BidiType = if start > 0 { types[start - 1] } else { sor };
        let after: BidiType = if end < utf16_len { types[end] } else { sor };
        let b_dir = if before != BidiType::L { BidiType::R } else { BidiType::L };
        let a_dir = if after  != BidiType::L { BidiType::R } else { BidiType::L };
        if b_dir == a_dir {
            for j in start..end { types[j] = b_dir; }
        }
        k = end;
    }
    // N2: remaining ON -> e.
    for k in 0..utf16_len {
        if types[k] == BidiType::On { types[k] = e; }
    }

    // I1-I2: candidate level computation.
    for k in 0..utf16_len {
        let t = types[k];
        if (levels[k] & 1) == 0 {
            if t == BidiType::R {
                levels[k] += 1;
            } else if t == BidiType::An || t == BidiType::En {
                levels[k] += 2;
            }
        } else if t == BidiType::L || t == BidiType::An || t == BidiType::En {
            levels[k] += 1;
        }
    }

    Some(levels)
}

/// `computeSegmentLevels(normalized, segStarts)` — pretext bidi.ts:160.
///
/// Returns per-segStarts `Vec<i8>` where each entry is the Unicode bidi
/// embedding level at the segment's starting UTF-16 code-unit index.
/// `None` if `normalized` has no RTL/AL/AN character (in which case the
/// caller treats all segments as LTR).
///
/// `seg_starts` are byte offsets into `normalized` (the `analysis.rs::starts[]`
/// contract); this fn translates each to a UTF-16 code-unit index by walking
/// `normalized.char_indices()` and accumulating each `char`'s UTF-16 length.
pub fn compute_segment_levels(normalized: &str, seg_starts: &[usize]) -> Option<Vec<i8>> {
    let bidi_levels = compute_bidi_levels(normalized)?;

    // Build byte_offset -> utf16_code_unit_index map by walking chars once.
    let mut byte_to_cu: Vec<(usize, usize)> = Vec::with_capacity(seg_starts.len());
    let mut cu_idx = 0usize;
    for (byte_off, ch) in normalized.char_indices() {
        if seg_starts.contains(&byte_off) {
            byte_to_cu.push((byte_off, cu_idx));
        }
        cu_idx += if (ch as u32) >= 0x10000 { 2 } else { 1 };
    }
    // Build result in caller's order, defaulting to 0 for off-the-end starts.
    let mut out: Vec<i8> = vec![0; seg_starts.len()];
    for (i, &req_byte) in seg_starts.iter().enumerate() {
        let cu = byte_to_cu
            .iter()
            .find(|(b, _)| *b == req_byte)
            .map(|(_, cu)| *cu)
            .unwrap_or(0);
        out[i] = bidi_levels.get(cu).copied().unwrap_or(0);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_basic_ascii() {
        assert_eq!(classify_code_point(b'A' as u32), BidiType::L);
        assert_eq!(classify_code_point(b'0' as u32), BidiType::En);
        assert_eq!(classify_code_point(b' ' as u32), BidiType::Ws); // space is WS
    }

    #[test]
    fn classify_hebrew_arabic() {
        // Hebrew Aleph U+05D0 — Word-class R; Arabic Alef U+0627 — AL.
        assert_eq!(classify_code_point(0x05D0), BidiType::R);
        assert_eq!(classify_code_point(0x0627), BidiType::Al);
        // Arabic-Indic digit zero U+0660 is AN.
        assert_eq!(classify_code_point(0x0660), BidiType::An);
    }

    #[test]
    fn compute_bidi_levels_pure_ltr_returns_none() {
        // No RTL/AL/AN character -> None (paragraph stays LTR).
        assert_eq!(compute_bidi_levels("Hello world 123"), None);
        assert_eq!(compute_bidi_levels(""), None);
    }

    #[test]
    fn compute_bidi_levels_pure_hebrew_returns_levels() {
        // Pure Hebrew: paragraph base direction R, every char R.
        let levels = compute_bidi_levels("שלום").expect("hebrew triggers bidi");
        assert_eq!(levels.len(), 4); // 4 chars × 1 cu each = 4
        // paragraph start_level = 1 (R); for pure R run under even base => I1: levels[0]==1
        // (levels start at 1, R stays 1).
        for &l in &levels { assert_eq!(l, 1); }
    }

    #[test]
    fn compute_bidi_levels_mixed_ltr_rtl() {
        // "abc שלום" = 3 L chars + space + 4 R chars = 8 cu
        let levels = compute_bidi_levels("abc שלום").expect("mixed triggers bidi");
        assert_eq!(levels.len(), 8);
        // Adjacent run layout: paragraph picks LTR (first strong is 'a' = L), startLevel=0.
        // Under LTR base: L stays 0 (level +0); R bumps to 1; space at idx 3 is WS→ON, neutral;
        // N1: ON between L (before) and R (after) — different dirs -> stays ON -> N2: ON→e (L). Level stays 0.
        assert_eq!(levels[0], 0); // 'a' L
        assert_eq!(levels[3], 0); // space
        assert_eq!(levels[4], 1); // first Hebrew R
        assert_eq!(levels[7], 1); // last Hebrew R
    }

    #[test]
    fn compute_segment_levels_byte_offset_indexing() {
        // Pass three segStarts (byte offsets) into `שֵ test`: Hbrew at bytes 0..2, etc.
        let s = "ש abc"; // Hbrew shin + ' ' + ' '+'a'+'b'+'c' = 7 utf8 bytes
        // byte 0: 'ש',  byte 2: ' ',  byte 3: 'a' — three seg starts
        let seg_starts = vec![0, 2, 3];
        let seg_levels = compute_segment_levels(s, &seg_starts).expect("hebrew triggers");
        // utf16: ש=1cu, ' '=1cu, 'a'=1cu, ... total 6 cu
        // cu_idx at byte 0 -> 0; byte 2 (' ') -> 1; byte 3 ('a') -> 2
        assert_eq!(seg_levels.len(), 3);
        // start_level = 1 (first strong is ש = R) so ש's level == 1, then 'a' under R base
        // (L base would be 0 for L char; but base is R so I-rules bump 'a' level).
        // Behaviorally we accept that seg_levels not all zero; verify Hbrew start is odd.
        assert_eq!(seg_levels[0] & 1, 1); // start of Hebrew run has odd level
    }
}
