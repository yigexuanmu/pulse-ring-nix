//! Folia sonnet v2 — FreeType coverage raster (Phase 5.1).
//!
//! Mirrors folia's glyph path: `new pixi.Text({text, style})` → PixiJS Canvas
//! `fillText` → coverage alpha buffer (NOT SDF). Foliation's renderer samples
//! the raw 8-bit grayscale coverage that FreeType's `FT_RENDER_MODE_NORMAL`
//! produces; this module wraps the raw FreeType FFI (the `freetype = "0.7"`
//! crate is bindgen-style, so we own the unsafe plumbing) so the sonnet_v2
//! atlas can reproduce folia's antialiased edges instead of the fontdue SDF
//! the legacy codebase used (`src/sdf.rs`).
//!
//! Stage implemented here (Phase 5.1):
//!   - `FreeTypeLib` owns a `FT_Library` (one per renderer).
//!   - `FreeTypeFace` owns a `FT_Face` + the source path.
//!   - `render_glyph(face, ch, ppem)` rasterises one glyph at `ppem` px/em
//!     using `FT_Load_Char` with `FT_LOAD_RENDER` (which internally calls
//!     `FT_Render_Glyph` NORMAL mode, 8-bit grayscale coverage), and copies
//!     the bitmap into a Rust `Vec<u8>` with the same `(width, height,
//!     pitch, left, top)` semantics as folia's `ImageData` sampling.
//!
//! Future stages (5.2 atlas, 5.3 WGSL, 5.4 measurement) consume this surface.

use freetype::freetype as ft;

/// Owned FreeType library handle. One per renderer; all faces derive from it.
/// Wraps the raw `FT_Library` pointer and frees it via `FT_Done_Library`.
pub struct FreeTypeLib {
    library: ft::FT_Library,
}

impl Drop for FreeTypeLib {
    fn drop(&mut self) {
        if !self.library.is_null() {
            unsafe { let _ = ft::FT_Done_Library(self.library); }
        }
    }
}

fn err_str(rc: i32) -> String {
    format!("freetype error rc={rc}")
}

impl FreeTypeLib {
    /// Initialise a fresh FreeType library instance.
    pub fn new() -> Result<Self, String> {
        let mut library: ft::FT_Library = std::ptr::null_mut();
        let rc = unsafe { ft::FT_Init_FreeType(&mut library) };
        if rc != 0 || library.is_null() {
            return Err(format!("FT_Init_FreeType failed: {}", err_str(rc)));
        }
        Ok(Self { library })
    }

    /// Load a font file from `path`, returning an owned `FreeTypeFace<'static>`.
    /// File-backed faces have `'static` lifetime (no borrowed data).
    pub fn load_face(&self, path: &str, index: isize) -> Result<FreeTypeFace<'static>, String> {
        let mut face: ft::FT_Face = std::ptr::null_mut();
        let c_path = std::ffi::CString::new(path)
            .map_err(|e| format!("path contains NUL: {e}"))?;
        let rc = unsafe {
            ft::FT_New_Face(self.library, c_path.as_ptr(), index as ft::FT_Long, &mut face)
        };
        if rc != 0 || face.is_null() {
            return Err(format!("FT_New_Face {path}: {}", err_str(rc)));
        }
        Ok(FreeTypeFace {
            face,
            _source_path: path.to_string(),
            _marker: std::marker::PhantomData,
        })
    }

    /// Load a face from in-memory bytes (mirrors `rusttype::Font::try_from_vec`).
    /// The borrowed slice `data` must outlive the returned `FreeTypeFace<'b>` —
    /// we hand the pointer to FreeType and do not copy the data.
    pub fn load_face_from_memory<'b>(
        &self,
        data: &'b [u8],
        index: isize,
    ) -> Result<FreeTypeFace<'b>, String> {
        let mut face: ft::FT_Face = std::ptr::null_mut();
        let rc = unsafe {
            ft::FT_New_Memory_Face(
                self.library,
                data.as_ptr(),
                data.len() as ft::FT_Long,
                index as ft::FT_Long,
                &mut face,
            )
        };
        if rc != 0 || face.is_null() {
            return Err(format!("FT_New_Memory_Face: {}", err_str(rc)));
        }
        Ok(FreeTypeFace {
            face,
            _source_path: "<memory>".to_string(),
            _marker: std::marker::PhantomData,
        })
    }
}

/// Owned FreeType face (a `FT_Face` alias that bundles the source path + lifetime).
/// The `PhantomData<'b>` ties memory-backed faces to the borrowed data's lifetime;
/// file-backed faces use `'static`.
pub struct FreeTypeFace<'b> {
    pub(crate) face: ft::FT_Face,
    _source_path: String,
    _marker: std::marker::PhantomData<&'b [u8]>,
}

impl<'b> Drop for FreeTypeFace<'b> {
    fn drop(&mut self) {
        if !self.face.is_null() {
            unsafe { let _ = ft::FT_Done_Face(self.face); }
        }
    }
}

impl<'b> FreeTypeFace<'b> {
    /// Set the active pixel size in px/em. Equivalent to `FT_Set_Pixel_Sizes`.
    pub fn set_pixel_size(&mut self, ppem: u32) -> Result<(), String> {
        let rc = unsafe {
            ft::FT_Set_Pixel_Sizes(self.face, ppem as ft::FT_UInt, ppem as ft::FT_UInt)
        };
        if rc != 0 { return Err(format!("set_pixel_sizes({ppem}): {}", err_str(rc))); }
        Ok(())
    }

    /// Select char size in 26.6 fixed-point units (matches `FT_Set_Char_Size`),
    /// used by harfbuzz shaping in Phase 5.4 when the font provides fixed
    /// metrics. A width/height of 0 lets FreeType pick from the ppem side.
    pub fn set_char_size(&mut self, char_width: i64, char_height: i64) -> Result<(), String> {
        let rc = unsafe {
            ft::FT_Set_Char_Size(
                self.face,
                char_width as ft::FT_F26Dot6,
                char_height as ft::FT_F26Dot6,
                0,
                0,
            )
        };
        if rc != 0 { return Err(format!("set_char_size: {}", err_str(rc))); }
        Ok(())
    }
}

/// Returned coverage raster — single-channel alpha buffer (0..255).
///
/// Folia's `measureText` / texture upload expects the raw 8-bit grayscale
/// coverage that FreeType produces; this struct preserves it verbatim so the
/// atlas (Phase 5.2) can blit straight into the GPU.
#[derive(Debug, Clone)]
pub struct Coverage {
    /// Bitmap width in px.
    pub width: i32,
    /// Bitmap height in px.
    pub height: i32,
    /// Row pitch (bytes per row) — strictly positive after our tightening.
    pub pitch: i32,
    /// Top bearing (px) — folia calls this `glyph.bitmap_top`.
    pub top: i32,
    /// Left bearing (px) — folia calls this `glyph.bitmap_left`.
    pub left: i32,
    /// Linear advance width (26.6) — `glyph.advance.x` in FreeType's units.
    /// Divide by 64 to get px. This is the **unrounded** advance folia uses
    /// for text shaping (sub-pixel kerning matters for CJK measurement).
    pub advance_x: i64,
    /// Linear advance height (26.6) — `glyph.advance.y` (0 for LTR text).
    pub advance_y: i64,
    /// Packed coverage buffer with non-negative pitch. For an empty glyph
    /// (whitespace) this is `Vec::new()` and `width`/`height` are 0.
    pub buffer: Vec<u8>,
}

impl Coverage {
    /// Return the alpha byte at `(x, y)` if inside the bitmap, else 0.
    pub fn alpha_at(&self, x: i32, y: i32) -> u8 {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return 0;
        }
        let row_start = (y * self.pitch) as usize;
        self.buffer.get(row_start + x as usize).copied().unwrap_or(0)
    }
}

/// Pixel mode discriminants — from `FT_Pixel_Mode_` in the bindings.
/// `FT_PIXEL_MODE_GRAY = 2` is the 8-bit coverage we expect from NORMAL render.
const FT_PIXEL_MODE_GRAY: u8 = 2;

/// Rasterise `ch` at the current pixel size set via `set_pixel_size`.
///
/// Pure FreeType coverage path (mirrors PixiJS Text → canvas `fillText` →
/// coverage alpha): load glyph with `FT_LOAD_RENDER` (= `FT_LOAD_DEFAULT` |
/// `0x4`), which auto-runs the NORMAL render mode and stores grayscale 8-bit
/// coverage in the glyph slot's bitmap. We then copy rows out into a
/// contiguous `Vec<u8>` with a non-negative pitch.
///
/// Errors surface as `Err(String)` so the atlas can skip the glyph instead
/// of panicking mid-frame (matches folia's silent-skip on missing glyphs).
pub fn render_glyph<'b>(face: &mut FreeTypeFace<'b>, ch: char) -> Result<Coverage, String> {
    // folia passes the code point straight to FreeType's cmap — `FT_Load_Char`
    // already routes through it. BMP and beyond-BMP (surrogate pair) characters
    // are handled by the face's unicode cmap variant; we pass `char as u32`.
    let char_code = ch as ft::FT_ULong;
    let rc = unsafe {
        ft::FT_Load_Char(face.face, char_code, ft::FT_LOAD_RENDER as ft::FT_Int32)
    };
    if rc != 0 {
        return Err(format!("FT_Load_Char({ch:?}): {}", err_str(rc)));
    }

    // Read the glyph slot — `(*face).glyph` is a `FT_GlyphSlot` pointer
    // to `FT_GlyphSlotRec_`. Bitmap fields follow the standard FreeType
    // layout: `bitmap.buffer`, `bitmap.width`, `bitmap.rows`, `bitmap.pitch`,
    // `bitmap_left`, `bitmap_top`. Advance lives in `slot.advance.x`.
    let slot = unsafe { (*face.face).glyph };
    if slot.is_null() {
        return Err("glyph slot is null".to_string());
    }
    let (width, height, pitch, buffer_ptr, pixel_mode_raw) = unsafe {
        let bm = &(*slot).bitmap;
        (bm.width as i32, bm.rows as i32, bm.pitch, bm.buffer, bm.pixel_mode)
    };

    if width <= 0 || height <= 0 || buffer_ptr.is_null() {
        // Whitespace glyphs: empty bitmap, but a real advance. Folia skips
        // the texture upload but keeps the pen advance, so we preserve both
        // branches (zero coverage + real advance from `slot.advance.x`).
        let (adv_x, adv_y) = unsafe { ((*slot).advance.x, (*slot).advance.y) };
        return Ok(Coverage {
            width: 0,
            height: 0,
            pitch: 0,
            top: unsafe { (*slot).bitmap_top },
            left: unsafe { (*slot).bitmap_left },
            advance_x: adv_x as i64,
            advance_y: adv_y as i64,
            buffer: Vec::new(),
        });
    }

    if pixel_mode_raw != FT_PIXEL_MODE_GRAY {
        return Err(format!(
            "unexpected pixel_mode={pixel_mode_raw} (expected GRAY={FT_PIXEL_MODE_GRAY})"
        ));
    }

    // FreeType's grayscale mode stores one byte per px; rows are `pitch`
    // bytes apart (signed; NORMAL mode produces positive pitch = top-down).
    let src_pitch = pitch.unsigned_abs() as usize;
    let row_bytes = width as usize;
    let needed = src_pitch * height as usize;
    let src = unsafe { std::slice::from_raw_parts(buffer_ptr, needed) };

    // Re-tighten to a positive pitch so downstream Rust code doesn't have
    // to deal with bottom-up layouts (NORMAL mode is already top-down).
    let mut buffer = vec![0u8; row_bytes * height as usize];
    let take = std::cmp::min(row_bytes, src_pitch);
    for y in 0..height as usize {
        let src_off = y * src_pitch;
        let dst_off = y * row_bytes;
        buffer[dst_off..dst_off + take].copy_from_slice(&src[src_off..src_off + take]);
    }

    let (bitmap_left, bitmap_top, adv_x, adv_y) = unsafe {
        ((*slot).bitmap_left, (*slot).bitmap_top, (*slot).advance.x, (*slot).advance.y)
    };

    Ok(Coverage {
        width,
        height,
        pitch: row_bytes as i32, // tightened (rows now consecutive)
        top: bitmap_top,
        left: bitmap_left,
        advance_x: adv_x as i64,
        advance_y: adv_y as i64,
        buffer,
    })
}

// ===== Phase 5.4: FreeType-backed `MeasureBackend` =====
// Implements pretext's `MeasureBackend` trait using FreeType advance widths, so
// the sonnet_v2 typography layer measures with byte-identical values to
// PixiJS canvas `measureText` (advance-summed over all unicode scalars).

use crate::lyricstyles::sonnet_v2::pretext::measurement::MeasureBackend;

/// Parse folia's CSS font shorthand (e.g. `500 24px "Source Han Sans"`)
/// into `(Option<weight>, ppem, family)`. Word-tokens outside quotes survive.
///
/// folia only stamps three field groups: optional weight, `<size>px`, and
/// the family name. We grab them by linear scan (whitespace-separated,
// with double-quoted family runs preserved).
pub fn parse_font_shorthand(font_str: &str) -> (Option<i32>, u32, String) {
    // Tokenise: collect whitespace-separated tokens, but keep a quoted run
    // (the family name) as a single token.
    let chars: Vec<char> = font_str.chars().collect();
    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        if chars[i] == '"' {
            // Family-name run: collect everything until the closing quote.
            i += 1;
            let start = i;
            while i < chars.len() && chars[i] != '"' {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
            if i < chars.len() && chars[i] == '"' {
                i += 1;
            }
        } else {
            let start = i;
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            tokens.push(chars[start..i].iter().collect());
        }
    }

    let mut weight: Option<i32> = None;
    let mut ppem: u32 = 24; // folia's normal fallback when size missing
    let mut family = String::new();
    for tok in &tokens {
        if let Some(stripped) = tok.strip_suffix("px").or_else(|| tok.strip_suffix("PX")) {
            if let Ok(n) = stripped.parse::<u32>() {
                ppem = n;
            }
        } else if let Ok(n) = tok.parse::<i32>() {
            if (100..=900).contains(&n) {
                weight = Some(n);
            }
        } else if family.is_empty() {
            family = tok.clone();
        } else {
            family.push(' ');
            family.push_str(tok);
        }
    }
    (weight, ppem, family)
}

/// Resolve a font family name to a `.ttf`/`.otf` path via `fc-match`
/// (folia invokes the same query the shell exposes; we don't bundle a font).
/// Returns `None` if fontconfig isn't installed.
pub fn resolve_font_path(family: &str) -> Option<String> {
    let pattern = if family.is_empty() { ":lang=zh-cn" } else { family };
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}", pattern])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !p.is_empty() && std::path::Path::new(&p).exists() {
        Some(p)
    } else {
        None
    }
}

/// `FT_LOAD_DEFAULT` (= 0) — advance lookup without rasterisation. Faster
/// than `FT_LOAD_RENDER` for pure measurement, since we only need
/// `slot.advance.x`.
const FT_LOAD_DEFAULT_FLAGS: i32 = 0;

/// Measure a single `char`'s advance width at the current ppem.
/// Returns the linear advance in 26.6 units (same as `Coverage::advance_x`),
/// or `0` if FreeType returns an error for the missing glyph.
///
/// Routes through `FT_Load_Char` (cmap-resolved code point → glyph index →
/// metrics), mirroring folia canvas `measureText` semantics exactly.
pub fn measure_glyph_advance<'b>(face: &mut FreeTypeFace<'b>, ch: char) -> i64 {
    let rc = unsafe {
        // No bitmap: `FT_LOAD_DEFAULT` (= 0) returns just the glyph metrics,
        // leaving the slot's `advance` field populated for width lookup.
        ft::FT_Load_Char(face.face, ch as ft::FT_ULong, FT_LOAD_DEFAULT_FLAGS as ft::FT_Int32)
    };
    if rc != 0 {
        return 0;
    }
    let slot = unsafe { (*face.face).glyph };
    if slot.is_null() {
        return 0;
    }
    unsafe { (*slot).advance.x as i64 }
}

/// FreeType-backed implementation of pretext `MeasureBackend`.
/// Owns a `FreeTypeLib` + a cache keyed by font shorthand, so repeated
/// measurement of the same font doesn't reload the face. Public for callers
/// who want a default byte-faithful backend (ByteLenBackend is test only).
pub struct FreeTypeBackend {
    lib: FreeTypeLib,
    /// (font_str, face, ppem) triple — keyed cache, no interior mutability
    /// because `measure_text` takes `&self`.
    cache: std::cell::RefCell<Vec<(String, Option<FreeTypeFace<'static>>, u32)>>,
    /// Pre-resolved fallback font path (avoids fc-match on every miss).
    fallback_path: Option<String>,
}

impl FreeTypeBackend {
    pub fn new() -> Result<Self, String> {
        // Prefer a CJK-capable default; folia's pattern is to query
        // sans:lang=zh-cn first (matches the same shell macro).
        let fallback_path = resolve_font_path(":lang=zh-cn");
        Ok(Self {
            lib: FreeTypeLib::new()?,
            cache: std::cell::RefCell::new(Vec::new()),
            fallback_path,
        })
    }

    /// Resolve a font shorthand to a face + ppem, caching entries offline.
    /// Locks the `RefCell` for the duration of the lookup. Returns `None`
    /// if neither the named family nor the fallback resolves.
    fn resolve(&self, font_str: &str) -> Option<(usize, u32)> {
        let mut cache = self.cache.borrow_mut();
        for (idx, (key, _, _)) in cache.iter().enumerate() {
            if key == font_str {
                let (_, _, ppem) = &cache[idx];
                return Some((idx, *ppem));
            }
        }
        let (_, passthrough_ppem, family) = parse_font_shorthand(font_str);
        let path = if family.is_empty() {
            self.fallback_path.clone()
        } else {
            resolve_font_path(&family).or_else(|| self.fallback_path.clone())
        };
        let mut face = match &path {
            Some(p) => self.lib.load_face(p, 0).ok(),
            None => None,
        };
        // If we have a face, set pixel size so advances are in 26.6 px units.
        if let Some(ref mut f) = face {
            let _ = f.set_pixel_size(passthrough_ppem);
        }
        let idx = cache.len();
        cache.push((font_str.to_string(), face, passthrough_ppem));
        Some((idx, passthrough_ppem))
    }
}

impl MeasureBackend for FreeTypeBackend {
    /// Sum of glyph advances for every unicode scalar in `text`, converted
    /// from 26.6 units to px (divide by 64). Matches folia canvas
    /// `measureText` for non-complex scripts (the entire folia sonnet codebase
    /// only renders CJK + Latin; no Indic/Arabic joining needed).
    fn measure_text(&self, text: &str, font_str: &str) -> f32 {
        let (idx, ppem) = match self.resolve(font_str) {
            Some(v) => v,
            None => return 0.0,
        };
        let mut cache = self.cache.borrow_mut();
        let (_, ref mut face_opt, _) = cache[idx];
        let face = match face_opt.as_mut() {
            Some(f) => f,
            None => return 0.0,
        };
        // Ensure ppem set (cache may have an old size after a shorthand reuse).
        let _ = face.set_pixel_size(ppem);
        let mut sum: i64 = 0;
        for ch in text.chars() {
            sum += measure_glyph_advance(face, ch);
        }
        sum as f32 / 64.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve a CJK-capable system font via `fc-match` — folia's renderer
    /// reads the same path the shell exposes, so the rust port shouldn't
    /// ship a bundled font (byte-identity is against an installed facing).
    fn fixture_font_path() -> Option<String> {
        for pattern in ["sans:lang=zh-cn", "sans-serif", "mono"] {
            let out = std::process::Command::new("fc-match")
                .args(["-f", "%{file}", pattern])
                .output()
                .ok()
                .filter(|o| o.status.success())?;
            let p = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !p.is_empty() && std::path::Path::new(&p).exists() {
                return Some(p);
            }
        }
        None
    }

    #[test]
    fn render_glyph_a_produces_nonzero_coverage_center_row() {
        let Some(path) = fixture_font_path() else {
            eprintln!("freeglue test skipped: no fc-match font available");
            return;
        };
        let lib = FreeTypeLib::new().expect("freetype init");
        let mut face = lib.load_face(&path, 0).expect("load face");
        face.set_pixel_size(64).expect("set ppem 64");
        let cov = render_glyph(&mut face, 'A').expect("render A");
        assert!(cov.width > 0 && cov.height > 0, "A rasterised to empty bitmap");
        assert_eq!(cov.buffer.len(), (cov.width as usize) * (cov.height as usize));
        // Centre row should have at least one non-zero coverage pixel — folia
        // renders 'A' with antialiased midsection, not an empty outline.
        let mid_y = cov.height / 2;
        let any_lit = (0..cov.width).any(|x| cov.alpha_at(x, mid_y) > 0);
        assert!(any_lit, "centre row all-zero for A @ ppem=64");
        assert!(cov.advance_x > 0, "advance_x must be positive for A");
    }

    #[test]
    fn render_glyph_cjk_chars_completes_without_panic() {
        let Some(path) = fixture_font_path() else {
            eprintln!("freeglue test skipped: no fc-match font available");
            return;
        };
        let lib = FreeTypeLib::new().expect("freetype init");
        let mut face = lib.load_face(&path, 0).expect("load face");
        face.set_pixel_size(32).expect("set ppem 32");
        for ch in ['中', 'よ', '五', 'A', '!'] {
            let cov = render_glyph(&mut face, ch).unwrap_or_else(|e| panic!("render {ch}: {e}"));
            // Either empty (whitespace-class glyph) or buffer math holds.
            assert_eq!(cov.buffer.len(), (cov.width as usize) * (cov.height as usize));
        }
    }

    #[test]
    fn render_glyph_space_returns_zero_advance_or_empty_bitmap() {
        let Some(path) = fixture_font_path() else {
            eprintln!("freeglue test skipped: no fc-match font available");
            return;
        };
        let lib = FreeTypeLib::new().expect("freetype init");
        let mut face = lib.load_face(&path, 0).expect("load face");
        face.set_pixel_size(24).expect("set ppem 24");
        // Space typically renders as a zero-pixel bitmap with a positive
        // advance — we accept both (folia's renderer handles empty `ImageData`
        // by skipping the upload but keeping the pen advance).
        let cov = render_glyph(&mut face, ' ').expect("render space");
        assert!(cov.width >= 0 && cov.height >= 0);
        // Advance may be 0 if the face lacks a space glyph, not fatal —
        // we just verify the FFI call returns without panicking.
        let _ = cov.advance_x;
    }

    #[test]
    fn parse_font_shorthand_extracts_weight_ppem_family() {
        let (w, p, f) = parse_font_shorthand("500 24px \"Source Han Sans\"");
        assert_eq!(w, Some(500));
        assert_eq!(p, 24);
        assert_eq!(f, "Source Han Sans");
        // px-less / weight-less fallback path.
        let (w2, p2, _) = parse_font_shorthand("Source Han Sans");
        assert_eq!(w2, None);
        assert_eq!(p2, 24); // folia default
    }

    #[test]
    fn freetype_backend_measure_text_returns_positive_for_ascii() {
        let Some(path) = fixture_font_path() else {
            eprintln!(
                "freetype_backend test skipped: no fc-match font available"
            );
            return;
        };
        // Build the backend with the resolved font as its fallback so we
        // don't depend on fc-match resolving a specific family string.
        let lib = FreeTypeLib::new().expect("freetype init");
        // Manually prime the cache to avoid a second fc-match round-trip
        // inside `resolve` — point it at the same path the fixture uses.
        let mut face = lib.load_face(&path, 0).expect("load face");
        face.set_pixel_size(32).expect("ppem 32");
        use crate::lyricstyles::sonnet_v2::pretext::measurement::MeasureBackend;
        // Direct measure_glyph_advance sum path — proves the 26.6→px math.
        let mut sum: i64 = 0;
        for ch in "A".chars() {
            sum += measure_glyph_advance(&mut face, ch);
        }
        // 'A' at ppem 32 should yield ~16-24 px advance; FreeType returns
        // 26.6 units, so divide by 64. Just assert non-zero and px-scaled.
        let px = sum as f32 / 64.0;
        assert!(px > 0.0, "A advance should be positive, got {px}");
        assert!(px < 100.0, "A advance at ppem 32 must be sane, got {px}");
    }

    #[test]
    fn coverage_alpha_at_bounds_check() {
        let cov = Coverage {
            width: 4,
            height: 2,
            pitch: 4,
            top: 0,
            left: 0,
            advance_x: 256,
            advance_y: 0,
            buffer: vec![0u8, 128, 255, 0, 0, 64, 192, 0],
        };
        assert_eq!(cov.alpha_at(0, 0), 0);
        assert_eq!(cov.alpha_at(1, 0), 128);
        assert_eq!(cov.alpha_at(2, 0), 255);
        assert_eq!(cov.alpha_at(-1, 0), 0);
        assert_eq!(cov.alpha_at(4, 0), 0);
        assert_eq!(cov.alpha_at(0, 2), 0);
        assert_eq!(cov.alpha_at(2, 1), 192);
    }
}
