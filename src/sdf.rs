//! SDF glyph atlas for real-time lyric text rendering.
//!
//! Glyphs are rasterised once (fontdue) into a signed distance field and packed into a
//! single-channel atlas. At render time each character becomes a quad that samples the SDF in
//! the shader, so text stays crisp at any zoom/rotation and supports glow/shadows. This is the
//! shared foundation the sonnet style (and future styles) draw from.

use std::collections::HashMap;

use fontdue::{Font, FontSettings, Metrics};

/// Rasterisation resolution (px) of a glyph's SDF. Higher = crisper under big upscales.
pub const RASTER_PX: f32 = 96.0;
/// Atlas cell size (px). Inner glyph area is `CELL - 2*PAD`.
pub const CELL: usize = 128;
/// SDF range: the distance field extends `PAD` px outside the glyph for AA + glow.
pub const PAD: usize = 16;
/// Grid size in cells per side. Four weight faces × a CJK song's unique chars can reach
/// ~1000 cells, so 18² was too tight (forced rebuild thrash on every new glyph).
pub const GRID: usize = 32;
/// Atlas pixel size = GRID * CELL.
pub const ATLAS_PX: usize = GRID * CELL;

/// A packed glyph: where its cell lives in the atlas and its font metrics at RASTER_PX.
#[derive(Debug, Clone, Copy)]
pub struct GlyphInfo {
    /// UV rect of the whole cell in the atlas (0..1).
    pub uv: [f32; 4],
    /// Rasterised glyph bitmap size (px) and baseline offset, at RASTER_PX.
    pub gw: f32,
    pub gh: f32,
    pub xmin: f32,
    pub ymin: f32,
    /// Advance width at RASTER_PX.
    pub advance: f32,
}

/// One laid-out character position at a given font size (px).
#[derive(Debug, Clone, Copy)]
pub struct PlacedChar {
    pub ch: char,
    /// Pen x at the requested size (px), kerning applied by fontdue.
    pub start: f32,
    /// Advance width at the requested size (px).
    pub advance: f32,
}

pub struct GlyphAtlas {
    /// Font 0 = regular, font 1 = bold (when available).
    fonts: Vec<Font>,
    cells: HashMap<(char, u8), GlyphInfo>,
    /// cell index -> key (for rebuild on overflow).
    cell_order: Vec<(usize, (char, u8))>,
    next_cell: usize,
    atlas: Vec<u8>,
    dirty: bool,
}

impl GlyphAtlas {
    pub fn new(regular_data: &[u8], bold_data: Option<&[u8]>) -> Result<Self, String> {
        Self::new_with_weights(regular_data, bold_data, None, None)
    }

    /// Four-weight atlas: regular(400) / bold(700) / black(900) / light(300). All but the
    /// first may be None (fall back gracefully, mirroring folia's role weights).
    pub fn new_with_weights(
        regular_data: &[u8],
        bold_data: Option<&[u8]>,
        black_data: Option<&[u8]>,
        light_data: Option<&[u8]>,
    ) -> Result<Self, String> {
        let mut fonts = Vec::with_capacity(4);
        let font = Font::from_bytes(
            regular_data.to_vec(),
            FontSettings {
                collection_index: 0,
                ..Default::default()
            },
        )
        .map_err(|e| format!("fontdue load failed: {e}"))?;
        fonts.push(font);
        let push = |fonts: &mut Vec<Font>, data: Option<&[u8]>| {
            if let Some(d) = data {
                if let Ok(f) = Font::from_bytes(d.to_vec(), FontSettings {
                    collection_index: 0,
                    ..Default::default()
                }) {
                    fonts.push(f);
                }
            }
        };
        push(&mut fonts, bold_data);
        push(&mut fonts, black_data);
        push(&mut fonts, light_data);
        Ok(Self {
            fonts,
            cells: HashMap::new(),
            cell_order: Vec::new(),
            next_cell: 0,
            atlas: vec![0u8; ATLAS_PX * ATLAS_PX],
            dirty: false,
        })
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// The single-channel atlas (one byte per pixel, SDF in 0..255, 0.5 = edge).
    pub fn atlas_bytes(&self) -> &[u8] {
        &self.atlas
    }

    /// Whether a bold font was loaded (fallback to regular otherwise).
    pub fn has_bold(&self) -> bool {
        self.fonts.len() > 1
    }

    fn font(&self, weight: u8) -> &Font {
        let idx = if (weight as usize) >= self.fonts.len() { 0 } else { weight as usize };
        &self.fonts[idx]
    }

    /// Ensure every character of `text` has a cell, rasterising new ones on demand.
    pub fn ensure_text(&mut self, text: &str, weight: u8) {
        for ch in text.chars() {
            self.ensure(ch, weight);
        }
    }

    pub fn ensure(&mut self, ch: char, weight: u8) {
        if ch == ' ' || ch == '\u{3000}' {
            return; // spaces don't need a glyph
        }
        let weight = if (weight as usize) >= self.fonts.len() { 0 } else { weight };
        let key = (ch, weight);
        if self.cells.contains_key(&key) {
            return;
        }
        if self.next_cell >= GRID * GRID {
            self.rebuild();
            if self.cells.contains_key(&key) {
                return;
            }
            // Atlas still full after the rebuild (more keys than cells): skip the pack
            // instead of writing past the end of the atlas buffer.
            if self.next_cell >= GRID * GRID {
                return;
            }
        }
        let font = self.font(weight);
        let (metrics, cov) = font.rasterize(ch, RASTER_PX);
        if metrics.width == 0 || metrics.height == 0 {
            self.pack_glyph(ch, weight, metrics, &cov);
            return;
        }
        self.pack_glyph(ch, weight, metrics, &cov);
    }

    fn pack_glyph(&mut self, ch: char, weight: u8, metrics: Metrics, cov: &[u8]) -> GlyphInfo {
        let idx = self.next_cell;
        self.next_cell += 1;
        let cx = (idx % GRID) * CELL;
        let cy = (idx / GRID) * CELL;

        // Build a padded cell: coverage placed at (PAD, PAD), outside = 0.
        let mut cellbuf = vec![0u8; CELL * CELL];
        for y in 0..metrics.height {
            for x in 0..metrics.width {
                let sy = PAD + y;
                let sx = PAD + x;
                if sy < CELL && sx < CELL {
                    cellbuf[sy * CELL + sx] = cov[y * metrics.width + x];
                }
            }
        }
        // Inside mask (coverage >= 128) → signed distance field.
        let inside: Vec<bool> = cellbuf.iter().map(|&c| c >= 128).collect();
        let dist = edt_signed(&inside, CELL, CELL);
        let range = PAD as f32;
        for (i, d) in dist.iter().enumerate() {
            // d > 0 inside, < 0 outside. Encode: 0.5 + d/(2*range) → 0..1.
            let v = (0.5 + d / (2.0 * range)).clamp(0.0, 1.0);
            self.atlas[cy * ATLAS_PX + cx + (i / CELL) * ATLAS_PX + (i % CELL)] = (v * 255.0) as u8;
        }

        let aw = ATLAS_PX as f32;
        let info = GlyphInfo {
            uv: [
                cx as f32 / aw,
                cy as f32 / aw,
                CELL as f32 / aw,
                CELL as f32 / aw,
            ],
            gw: metrics.width as f32,
            gh: metrics.height as f32,
            xmin: metrics.xmin as f32,
            ymin: metrics.ymin as f32,
            advance: metrics.advance_width,
        };
        self.cells.insert((ch, weight), info);
        self.cell_order.push((idx, (ch, weight)));
        self.dirty = true;
        info
    }

    /// When the atlas is full, rebuild with the current glyph set (rare; a song's char set
    /// stabilises quickly).
    fn rebuild(&mut self) {
        let keep: Vec<(char, u8)> = self.cells.keys().copied().collect();
        self.cells.clear();
        self.cell_order.clear();
        self.next_cell = 0;
        self.atlas.iter_mut().for_each(|b| *b = 0);
        for (ch, weight) in keep {
            if self.next_cell < GRID * GRID {
                self.ensure(ch, weight);
            }
        }
    }

    /// Lay out `text` at `size_px`, returning per-character pen positions (kerning applied).
    pub fn layout(&self, text: &str, size_px: f32, weight: u8) -> Vec<PlacedChar> {
        let font = self.font(weight);
        let mut out = Vec::new();
        let mut pen = 0.0f32;
        let mut prev: Option<char> = None;
        for ch in text.chars() {
            if let Some(p) = prev {
                if let Some(k) = font.horizontal_kern(p, ch, size_px) {
                    pen += k;
                }
            }
            let m = font.metrics(ch, size_px);
            out.push(PlacedChar { ch, start: pen, advance: m.advance_width });
            pen += m.advance_width;
            prev = Some(ch);
        }
        out
    }

    /// Total advance width of `text` at `size_px`.
    pub fn measure(&self, text: &str, size_px: f32, weight: u8) -> f32 {
        self.layout(text, size_px, weight).iter().map(|p| p.advance).sum()
    }

    pub fn glyph(&self, ch: char, weight: u8) -> Option<&GlyphInfo> {
        let weight = if (weight as usize) >= self.fonts.len() { 0 } else { weight };
        self.cells.get(&(ch, weight))
    }

    /// Number of glyphs currently packed.
    pub fn glyph_count(&self) -> usize {
        self.cells.len()
    }
}

/// Exact Euclidean distance transform (Felzenszwalb & Huttenlocher) over a binary mask,
/// returning signed distances: positive inside, negative outside (in px).
fn edt_signed(inside: &[bool], w: usize, h: usize) -> Vec<f32> {
    let n = w * h;
    let large = 1e6f32;
    let mut f = vec![0f32; n];
    for i in 0..n {
        f[i] = if inside[i] { 0.0 } else { large };
    }
    // Columns.
    let mut work = vec![0f32; h.max(w)];
    let mut col = vec![0f32; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = f[y * w + x];
        }
        edt_1d(&col, &mut work, h);
        for y in 0..h {
            f[y * w + x] = work[y];
        }
    }
    // Rows.
    let mut row = vec![0f32; w];
    for y in 0..h {
        for x in 0..w {
            row[x] = f[y * w + x];
        }
        edt_1d(&row, &mut work, w);
        for x in 0..w {
            f[y * w + x] = work[x];
        }
    }
    // Signed: sqrt inside, -sqrt outside.
    let mut out = vec![0f32; n];
    for i in 0..n {
        let d = f[i].sqrt();
        out[i] = if inside[i] { d } else { -d };
    }
    out
}

/// 1D squared-distance transform of `f` (Felzenszwalb-Huttenlocher). `out` gets `n` entries.
fn edt_1d(f: &[f32], out: &mut [f32], n: usize) {
    let mut v = vec![0usize; n];
    let mut z = vec![0f32; n + 1];
    let inf = f32::INFINITY;
    let mut k = 0usize;
    v[0] = 0;
    z[0] = -inf;
    z[1] = inf;
    for q in 1..n {
        let mut s = ((f[q] + q as f32 * q as f32) - (f[v[k]] + v[k] as f32 * v[k] as f32))
            / (2.0 * q as f32 - 2.0 * v[k] as f32);
        while s <= z[k] {
            k -= 1;
            s = ((f[q] + q as f32 * q as f32) - (f[v[k]] + v[k] as f32 * v[k] as f32))
                / (2.0 * q as f32 - 2.0 * v[k] as f32);
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = inf;
    }
    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        out[q] = (q as f32 - v[k] as f32) * (q as f32 - v[k] as f32) + f[v[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolve any usable font file via fontconfig (test envs may lack the hard-coded paths).
    fn font_bytes() -> Vec<u8> {
        for pattern in ["sans", "mono", "serif"] {
            if let Ok(out) = std::process::Command::new("fc-match")
                .args(["-f", "%{file}\n", pattern])
                .output()
            {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if let Ok(data) = std::fs::read(&path) {
                    return data;
                }
            }
        }
        panic!("no test font via fc-match");
    }

    #[test]
    fn packs_and_measures() {
        let data = font_bytes();
        let mut atlas = GlyphAtlas::new(&data, None).unwrap();
        atlas.ensure_text("Hello 你好", 0);
        assert!(atlas.is_dirty());
        atlas.clear_dirty();
        assert!(atlas.glyph('H', 0).is_some());
        assert!(atlas.glyph('你', 0).is_some() || atlas.glyph('好', 0).is_some());
        // Re-ensuring is idempotent and doesn't dirty.
        atlas.ensure_text("Hello 你好", 0);
        assert!(!atlas.is_dirty());
        let w = atlas.measure("Hello", 48.0, 0);
        assert!(w > 0.0);
        let placed = atlas.layout("Hi", 48.0, 0);
        assert_eq!(placed.len(), 2);
        // Layout positions are monotonically increasing.
        assert!(placed[1].start >= placed[0].start);
    }

    #[test]
    fn demo_text_cells_not_solid() {
        let data = font_bytes();
        let mut atlas = GlyphAtlas::new(&data, None).unwrap();
        let demo = "这是一段歌词预览 第二行用于测试换行 Third line of the sample 第四行继续展示动画 最后一行淡出结束 A quiet night fades away 夜空闪烁的星光";
        atlas.ensure_text(demo, 0);
        let aw = ATLAS_PX as f32;
        for ch in demo.chars() {
            if ch == ' ' {
                continue;
            }
            let Some(info) = atlas.glyph(ch, 0) else { continue };
            let x0 = (info.uv[0] * aw) as usize;
            let y0 = (info.uv[1] * aw) as usize;
            let w = (info.uv[2] * aw) as usize;
            let h = (info.uv[3] * aw) as usize;
            let mut hi = 0usize;
            let mut n = 0usize;
            for yy in 0..h.min(CELL) {
                for xx in 0..w.min(CELL) {
                    let v = atlas.atlas[(y0 + yy) * ATLAS_PX + x0 + xx];
                    n += 1;
                    if v > 160 {
                        hi += 1;
                    }
                }
            }
            let frac = hi as f32 / n.max(1) as f32;
            assert!(frac < 0.9, "glyph {:?} cell nearly solid (frac={:.2})", ch, frac);
        }
    }

    #[test]
    fn edt_is_signed() {
        // 1x3: [outside, inside, outside]. Boundary pixel has distance 0 (the edge).
        let inside = vec![false, true, false];
        let d = edt_signed(&inside, 3, 1);
        assert!(d[1] >= 0.0, "inside should be >= 0, got {}", d[1]);
        assert!(d[0] <= 0.0 && d[2] <= 0.0, "outside should be <= 0, got {} {}", d[0], d[2]);
        // Outside values are closer to 0 than the inside value.
        assert!(d[1] > d[0] && d[1] > d[2]);
    }
}
