//! Headless software preview of the lyric layer.
//!
//! `pulse-ring preview "<text>" [style] [time]` renders the lyric frame with the same layout
//! math as the WGSL layer (SDF glyph sampling), composites onto an RGBA canvas and writes a
//! PNG, so visuals can be checked without a Wayland/GPU session.

use crate::lyricview::{CharQuad, LyricColors, SLOT_PILL, StyleCtx, StyleInput};
use crate::sdf::GlyphAtlas;

const W: u32 = 1920;
const H: u32 = 1080;

pub fn render(text: &str, style: &str, time: f32, path: &str) -> Result<(), String> {
    let font_data = crate::load_font_data();
    if font_data.is_empty() {
        return Err("no system font found".into());
    }
    let mut atlas = GlyphAtlas::new(&font_data, None)?;

    // Hidden debug: dump a glyph's SDF cell as ASCII so glyph shape can be verified.
    if std::env::var("PULSE_RING_DUMP_GLYPH").is_ok() {
        let ch = text.chars().next().unwrap_or('A');
        atlas.ensure(ch, 0);
        let info = atlas.glyph(ch, 0).ok_or("no glyph")?;
        let aw = crate::sdf::ATLAS_PX as f32;
        let (u0, v0, uw, vh) = (info.uv[0], info.uv[1], info.uv[2], info.uv[3]);
        let chars = " .:-=+*#%@";
        for gy in 0..20 {
            let mut row = String::new();
            for gx in 0..48 {
                let u = u0 + (gx as f32 + 0.5) / 48.0 * uw;
                let v = v0 + (gy as f32 + 0.5) / 20.0 * vh;
                let d = sample_sdf(&atlas, u, v);
                let c = if d > 0.55 { '#' } else if d > 0.5 { '+' } else if d > 0.45 { '.' } else { ' ' };
                row.push(c);
            }
            println!("{row}");
        }
        return Ok(());
    }

    let mut canvas = vec![0u8; (W * H * 4) as usize];

    // Synthetic lyric data: a few lines so sonnet can build a shot, with a translation.
    let mk = |start_ms: i64, text: &str, translation: &str| crate::lyrics::LyricLine {
        start_ms,
        duration_ms: 3500,
        text: text.to_string(),
        translation: translation.to_string(),
        romanization: String::new(),
        chars: vec![],
    };
    let lines = if text == "-" {
        // Neutral placeholder (no real-song lyrics) so preview still works without a text arg.
        vec![
            mk(1000, "这是一段歌词预览", "A lyric preview line"),
            mk(5000, "第二行用于测试换行", "Second line wraps for testing"),
            mk(9500, "Third line of the sample", "第三行示例歌词"),
            mk(14000, "第四行继续展示动画", "The fourth line keeps animating"),
            mk(18500, "最后一行淡出结束", "The final line fades out"),
        ]
    } else {
        vec![
            mk(1000, text, "夜空闪烁的星光"),
            mk(6000, "第二行是副歌的高潮", "这是第二行的翻译"),
            mk(11000, "A quiet night fades away", ""),
        ]
    };
    let active_idx = 0usize;
    for l in &lines {
        atlas.ensure_text(&l.text, 0);
        atlas.ensure_text(&l.translation, 0);
    }

    let colors = LyricColors::default();
    let ctx = StyleCtx {
        width: W as f32,
        height: H as f32,
        time,
        atlas: &atlas,
        colors: &colors,
        seed: 0x1234_5678,
        mg_bg: true,
        mg_fixed: true,
        mg_decor: true,
        audio: [0.3, 0.3, 0.3],
        post: [0.3, 0.5, 0.4, 0.6, 0.3, 0.3, 0.5],
        font_weight: 0.0,
    };
    let input = StyleInput {
        lines: &lines,
        active_idx,
        translation: &lines[0].translation,
        song_title: "预览歌曲",
        song_artist: "Preview Artist",
        song_album: "",
    };
    let parsed_style = match style {
        "sonnet" | "商籁" => crate::config::LyricStyle::Sonnet,
        "classic" | "经典" | "luminous" | "流动" => crate::config::LyricStyle::Classic,
        _ => crate::config::LyricStyle::Off,
    };
    let output = crate::lyricview::build_frame(parsed_style, &ctx, &input);
    let quads = output.quads;
    if std::env::var("PULSE_RING_DEBUG_PREVIEW").is_ok() {
        eprintln!("preview: style={style} time={time} quads={} fx={:?}", quads.len(), output.fx.to_array());
    }

    if std::env::var("PULSE_RING_DEBUG_PREVIEW").is_ok() {
        for q in &quads {
            eprintln!(
                "quad glow={} uv=({:.2},{:.2},{:.2},{:.2}) px=({:.0},{:.0}) pos=({:.0},{:.0}) scale={:.2} alpha={:.2} rot={:.2}",
                q.glow, q.uv[0], q.uv[1], q.uv[2], q.uv[3], q.px[0], q.px[1], q.pos[0], q.pos[1], q.scale, q.alpha, q.rotate
            );
        }
    }

    for q in &quads {
        if q.glow >= SLOT_PILL {
            draw_pill(&mut canvas, q);
        } else if q.glow >= crate::lyricview::SLOT_FRAME {
            draw_frame(&mut canvas, q);
        } else if q.glow >= crate::lyricview::SLOT_TRI {
            draw_triangle(&mut canvas, q);
        } else {
            draw_glyph(&mut canvas, q, &atlas);
        }
    }

    let img = image::RgbaImage::from_raw(W, H, canvas).ok_or("bad canvas")?;
    img.save(path).map_err(|e| format!("save failed: {e}"))?;
    Ok(())
}

/// SDF glyph quad: sample the atlas, apply coverage smoothstep (same as the shader).
fn draw_glyph(canvas: &mut [u8], q: &CharQuad, atlas: &GlyphAtlas) {
    if q.alpha <= 0.004 {
        return;
    }
    let half = [q.px[0] * q.scale * 0.5, q.px[1] * q.scale * 0.5];
    let (cs, sn) = ((-q.rotate).cos(), (-q.rotate).sin());
    let x0 = (q.pos[0] - half[0]).max(0.0) as u32;
    let x1 = ((q.pos[0] + half[0]).min(W as f32)) as u32;
    let y0 = (q.pos[1] - half[1]).max(0.0) as u32;
    let y1 = ((q.pos[1] + half[1]).min(H as f32)) as u32;
    let (u0, v0, uw, vh) = (q.uv[0], q.uv[1], q.uv[2], q.uv[3]);
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 - q.pos[0];
            let dy = py as f32 - q.pos[1];
            let llx = dx * cs - dy * sn;
            let lly = dx * sn + dy * cs;
            if llx.abs() > half[0] || lly.abs() > half[1] {
                continue;
            }
            let u = u0 + (llx / (half[0] * 2.0) + 0.5) * uw;
            let v = v0 + (lly / (half[1] * 2.0) + 0.5) * vh;
            let d = sample_sdf(atlas, u, v);
            let s = q.px[0] / 128.0;
            let aa = (1.0 / (32.0 * s)).clamp(0.004, 0.25);
            let cov = smoothstep(0.5 - aa, 0.5 + aa, d);
            // Per-quad chromatic aberration: re-sample R/B edges at a small offset.
            let mut cov_rgb = cov;
            if q.ext[0] > 0.001 {
                let ca = q.ext[0] * (uw * 0.04);
                let d_r = sample_sdf(atlas, u + ca, v);
                let d_b = sample_sdf(atlas, u - ca, v);
                let cov_r = smoothstep(0.5 - aa, 0.5 + aa, d_r);
                let cov_b = smoothstep(0.5 - aa, 0.5 + aa, d_b);
                cov_rgb = (cov_r + cov + cov_b) * (1.0 / 3.0);
            }
            let mut a = cov_rgb * q.color[3] * q.alpha;
            let mut r = q.color[0];
            let mut g = q.color[1];
            let mut b = q.color[2];
            if q.glow > 0.0 {
                let d2 = sample_sdf(atlas, u, v);
                // Outside glow halo: peaks at the glyph edge (SDF 0.5) and fades outward over
                // `band` (~4px). Zero far from the glyph, so empty space never boxes.
                let band = 0.12;
                let ga = smoothstep(0.5 - band, 0.5, d2) * q.glow;
                let acc = a + ga * (1.0 - a);
                if acc > 0.004 {
                    r = (r * a + r * ga * (1.0 - a)) / acc;
                    g = (g * a + g * ga * (1.0 - a)) / acc;
                    b = (b * a + b * ga * (1.0 - a)) / acc;
                    a = acc;
                }
            }
            if a <= 0.004 {
                continue;
            }
            blend(canvas, px, py, r * a, g * a, b * a, a);
        }
    }
}

fn sample_sdf(atlas: &GlyphAtlas, u: f32, v: f32) -> f32 {
    let x = (u.clamp(0.0, 1.0) * crate::sdf::ATLAS_PX as f32) as usize;
    let y = (v.clamp(0.0, 1.0) * crate::sdf::ATLAS_PX as f32) as usize;
    if x >= crate::sdf::ATLAS_PX || y >= crate::sdf::ATLAS_PX {
        return 0.0;
    }
    atlas.atlas_bytes()[y * crate::sdf::ATLAS_PX + x] as f32 / 255.0
}

/// Rounded-rect pill background (translation bar).
fn draw_pill(canvas: &mut [u8], q: &CharQuad) {
    if q.alpha <= 0.004 {
        return;
    }
    let half = [q.px[0] * 0.5 * q.scale, q.px[1] * 0.5 * q.scale];
    let r = q.px[0].min(q.px[1]) * 0.5;
    let x0 = (q.pos[0] - half[0]).max(0.0) as u32;
    let x1 = ((q.pos[0] + half[0]).min(W as f32)) as u32;
    let y0 = (q.pos[1] - half[1]).max(0.0) as u32;
    let y1 = ((q.pos[1] + half[1]).min(H as f32)) as u32;
    for py in y0..y1 {
        for px in x0..x1 {
            let d = [(px as f32 - q.pos[0]).abs(), (py as f32 - q.pos[1]).abs()];
            let d = [d[0] - (half[0] - r), d[1] - (half[1] - r)];
            let qd = [d[0].max(0.0), d[1].max(0.0)];
            let sd = (qd[0] * qd[0] + qd[1] * qd[1]).sqrt() + d[0].max(d[1]).min(0.0) - r;
            let a = smoothstep(1.5, -1.5, sd) * q.color[3] * q.alpha;
            if a <= 0.004 {
                continue;
            }
            blend(canvas, px, py, q.color[0] * a, q.color[1] * a, q.color[2] * a, a);
        }
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Rotated low-corner-radius rect (frame bars / line segments).
fn draw_frame(canvas: &mut [u8], q: &CharQuad) {
    if q.alpha <= 0.004 {
        return;
    }
    let half = [q.px[0] * 0.5 * q.scale, q.px[1] * 0.5 * q.scale];
    let r = q.px[0].min(q.px[1]) * 0.12;
    let (cs, sn) = ((-q.rotate).cos(), (-q.rotate).sin());
    let x0 = (q.pos[0] - half[0] - 2.0).max(0.0) as u32;
    let x1 = ((q.pos[0] + half[0] + 2.0).min(W as f32)) as u32;
    let y0 = (q.pos[1] - half[1] - 2.0).max(0.0) as u32;
    let y1 = ((q.pos[1] + half[1] + 2.0).min(H as f32)) as u32;
    for py in y0..y1 {
        for px in x0..x1 {
            let dx = px as f32 - q.pos[0];
            let dy = py as f32 - q.pos[1];
            let llx = dx * cs - dy * sn;
            let lly = dx * sn + dy * cs;
            let d = [llx.abs(), lly.abs()];
            let d = [d[0] - (half[0] - r), d[1] - (half[1] - r)];
            let qd = [d[0].max(0.0), d[1].max(0.0)];
            let sd = (qd[0] * qd[0] + qd[1] * qd[1]).sqrt() + d[0].max(d[1]).min(0.0) - r;
            let a = smoothstep(1.5, -1.5, sd) * q.color[3] * q.alpha;
            if a <= 0.004 {
                continue;
            }
            blend(canvas, px, py, q.color[0] * a, q.color[1] * a, q.color[2] * a, a);
        }
    }
}

/// Filled triangle: vertices are stage-local px relative to the quad centre.
fn draw_triangle(canvas: &mut [u8], q: &CharQuad) {
    if q.alpha <= 0.004 {
        return;
    }
    let v0 = [q.ext[0], q.ext[1]];
    let v1 = [q.ext[2], q.ext[3]];
    let v2 = [q.uv[0], q.uv[1]];
    let bx = q.px[0] * 0.5 * q.scale + 2.0;
    let by = q.px[1] * 0.5 * q.scale + 2.0;
    let x0 = (q.pos[0] - bx).max(0.0) as u32;
    let x1 = ((q.pos[0] + bx).min(W as f32)) as u32;
    let y0 = (q.pos[1] - by).max(0.0) as u32;
    let y1 = ((q.pos[1] + by).min(H as f32)) as u32;
    for py in y0..y1 {
        for px in x0..x1 {
            let p = [px as f32 - q.pos[0], py as f32 - q.pos[1]];
            let sd = sd_tri(v0, v1, v2, p);
            let a = smoothstep(1.5, -1.5, sd) * q.color[3] * q.alpha;
            if a <= 0.004 {
                continue;
            }
            blend(canvas, px, py, q.color[0] * a, q.color[1] * a, q.color[2] * a, a);
        }
    }
}

fn sd_tri(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p: [f32; 2]) -> f32 {
    let e0 = [p1[0] - p0[0], p1[1] - p0[1]];
    let e1 = [p2[0] - p1[0], p2[1] - p1[1]];
    let e2 = [p0[0] - p2[0], p0[1] - p2[1]];
    let v0 = [p[0] - p0[0], p[1] - p0[1]];
    let v1 = [p[0] - p1[0], p[1] - p1[1]];
    let v2 = [p[0] - p2[0], p[1] - p2[1]];
    let pq = |e: [f32; 2], v: [f32; 2]| {
        let t = ((v[0] * e[0] + v[1] * e[1]) / (e[0] * e[0] + e[1] * e[1])).clamp(0.0, 1.0);
        [v[0] - e[0] * t, v[1] - e[1] * t]
    };
    let pq0 = pq(e0, v0);
    let pq1 = pq(e1, v1);
    let pq2 = pq(e2, v2);
    let s = (e0[0] * e2[1] - e0[1] * e2[0]).signum();
    let d0 = [pq0[0] * pq0[0] + pq0[1] * pq0[1], s * (v0[0] * e0[1] - v0[1] * e0[0])];
    let d1 = [pq1[0] * pq1[0] + pq1[1] * pq1[1], s * (v1[0] * e1[1] - v1[1] * e1[0])];
    let d2 = [pq2[0] * pq2[0] + pq2[1] * pq2[1], s * (v2[0] * e2[1] - v2[1] * e2[0])];
    let x = d0[0].min(d1[0]).min(d2[0]);
    let y = d0[1].min(d1[1]).min(d2[1]);
    -x.sqrt() * y.signum()
}

/// Premultiplied over compositing onto the canvas.
fn blend(canvas: &mut [u8], px: u32, py: u32, r: f32, g: f32, b: f32, a: f32) {
    let o = ((py * W + px) * 4) as usize;
    let da = canvas[o + 3] as f32 / 255.0;
    let out_a = a + da * (1.0 - a);
    if out_a <= 0.004 {
        return;
    }
    let dr = canvas[o] as f32 / 255.0;
    let dg = canvas[o + 1] as f32 / 255.0;
    let db = canvas[o + 2] as f32 / 255.0;
    canvas[o] = (((r + dr * da * (1.0 - a)) / out_a) * 255.0) as u8;
    canvas[o + 1] = (((g + dg * da * (1.0 - a)) / out_a) * 255.0) as u8;
    canvas[o + 2] = (((b + db * da * (1.0 - a)) / out_a) * 255.0) as u8;
    canvas[o + 3] = (out_a * 255.0) as u8;
}
