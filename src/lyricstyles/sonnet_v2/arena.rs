//! Foliation sonnet v2 — arena scene graph (Phase 6.1, X-architecture core).
//!
//! Byte-identical 1:1 port of folia `sonnetSceneBuilder.ts` / `sonnetTextViewBuilder.ts`
//! view-graph contracts, replacing PIXI `Container`/`Text`/`Filter` handles with
//! stable integer arena slot indices so a persistent scene graph can be mutated
//! each frame and flattened to `Vec<CharQuad>` for `draw.rs` without owning GPU
//! resources.
//!
//! Each folia view interface (`GlyphView` / `SegmentView` / `ShotView` / `SceneView`)
//! maps to a Rust node struct with the *exact* same field set. PIXI live objects and
//! closures (`updateAnimation`, `ghosts`) are lowered to:
//!   - arena slot indices (`SlotId`) for things that were PIXI display objects, or
//!   - precomputed envelope state (`GhostEnvelope`, `GlyphAnimationState`) for
//!     things that were closures, so the runtime can re-evaluate without fn-ptrs.
//!
//! This file is intentionally side-effect free (no GPU allocation, no atlas
//! mutation). It owns the *shape* of the scene graph only.

use crate::lyricstyles::sonnet_v2::types::{
    SonnetParagraph, SonnetSegmentRole, SonnetShot, SonnetShotKind,
};

// ============================================================================
// Index newtypes — stable integer slot ids into the arena.
// ----------------------------------------------------------------------------
// Folia holds display objects directly (`container: PIXI.Container`); the
// Rust port holds an index into `SceneArena` vectors so mut-borrow churn
// is avoided (one mutable pass over `&mut Vec<Node>` each frame).
// ============================================================================

/// Opaque slot index for a scene-graph node (glyph display, mg layer, filter…).
/// `u32::MAX` reserved as the null sentinel (mirrors folia `null`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SlotId(pub u32);

impl SlotId {
    pub const NULL: SlotId = SlotId(u32::MAX);
    pub fn is_null(self) -> bool {
        self.0 == Self::NULL.0
    }
}

// ============================================================================
// Envelope / animation state — replaces folia closures.
// ----------------------------------------------------------------------------
// folia `GlyphView.updateAnimation?: (time: number) => void` is a closure the
// runtime calls each frame; the Rust port stores the *ingredients* the closure
// would read so the runtime re-evaluates deterministically. Phase 6.2 lands the
// actual runtime consumer; this file only declares the carrier structs.
// ============================================================================

/// Precomputed per-ghost spread (folia `GlyphGhostView` minus the PIXI `Text`).
#[derive(Clone, Debug, Default)]
pub struct GhostEnvelope {
    /// Full-spread offset in wrapper-local px (folia `dirX`/`dirY`).
    pub dir_x: f64,
    pub dir_y: f64,
    /// Layer peak alpha, precomputed (folia `alphaBase`).
    pub alpha_base: f64,
}

/// Carrier for the closure state folia tucks into `GlyphView.updateAnimation`.
/// Phase 6.2 fills this with the resolved motion curve (timeline-shake /
/// camera-breath) — declared here so `GlyphNode` can hold it by value.
#[derive(Clone, Debug, Default)]
pub struct GlyphAnimationState {
    /// Resolved amplitude scale (0..1) the runtime multiplies the envelope by.
    pub envelope_scale: f64,
    /// Resolved phase offset (radians) for the timeline-shake cosine.
    pub phase_offset: f64,
}

// ============================================================================
// Glyph node — faithful port of folia `GlyphView`.
// ----------------------------------------------------------------------------
// PIXI handle (`display: PIXI.Container`) → `display_slot: SlotId`.
// Optional PIXI `Text` fields (`halo`, `caCyan`, `caRed`) → `Option<SlotId>`.
// `ghosts?: GlyphGhostView[]` → `Vec<GhostEnvelope>` (no PIXI inside ghosts).
// `updateAnimation?` → `GlyphAnimationState` (no `FnMut`).
// ============================================================================

/// Arena node carrying the exact field set of folia `GlyphView`.
#[derive(Clone, Debug)]
pub struct GlyphNode {
    /// folia `display: PIXI.Container` — main glyph quad container.
    pub display_slot: SlotId,
    /// folia `halo: PIXI.Text | null`.
    pub halo_slot: Option<SlotId>,
    /// folia `caCyan?: PIXI.Text`.
    pub ca_cyan_slot: Option<SlotId>,
    /// folia `caRed?: PIXI.Text`.
    pub ca_red_slot: Option<SlotId>,
    /// folia `caOffset?: number`.
    pub ca_offset: Option<f64>,
    /// folia `ghosts?: GlyphGhostView[]` — envelopes carry no PIXI handle.
    pub ghosts: Vec<GhostEnvelope>,
    /// folia `ghostDuration?: number`.
    pub ghost_duration: Option<f64>,
    /// folia `baseX: number`.
    pub base_x: f64,
    /// folia `baseY: number`.
    pub base_y: f64,
    /// folia `enterX: number`.
    pub enter_x: f64,
    /// folia `enterY: number`.
    pub enter_y: f64,
    /// folia `entryRotation: number`.
    pub entry_rotation: f64,
    /// folia `finalRotation: number`.
    pub final_rotation: f64,
    /// folia `startTime: number`.
    pub start_time: f64,
    /// folia `settleTime: number`.
    pub settle_time: f64,
    /// folia `zDepth: number`.
    pub z_depth: f64,
    /// folia `isBackgroundShape?: boolean`.
    pub is_background_shape: bool,
    /// folia `isTextGlyph?: boolean`.
    pub is_text_glyph: bool,
    /// Precomputed carrier for folia `updateAnimation` closure state.
    pub animation: GlyphAnimationState,
}

impl Default for GlyphNode {
    fn default() -> Self {
        Self {
            display_slot: SlotId::NULL,
            halo_slot: None,
            ca_cyan_slot: None,
            ca_red_slot: None,
            ca_offset: None,
            ghosts: Vec::new(),
            ghost_duration: None,
            base_x: 0.0,
            base_y: 0.0,
            enter_x: 0.0,
            enter_y: 0.0,
            entry_rotation: 0.0,
            final_rotation: 0.0,
            start_time: 0.0,
            settle_time: 0.0,
            z_depth: 0.0,
            is_background_shape: false,
            is_text_glyph: false,
            animation: GlyphAnimationState::default(),
        }
    }
}

// ============================================================================
// Segment node — faithful port of folia `SegmentView`.
// ----------------------------------------------------------------------------
// `glyphs: GlyphView[]` and `trackingGlyphs: GlyphView[]` become slices of
// `GlyphNode` indices into the arena (so the glyph vector is flat-owned).
// `guide: SonnetGuideView`, `frameDecor?: SonnetFrameDecorView` are forward
// types from siblings not yet ported (Phase 6.2 Mg family); declared as
// `SlotId` sentinel placeholders so the field set matches folia's shape and
// the missing siblings can attach their real node structs without reshaping
// the segment node.
// ============================================================================

/// Arena node carrying the exact field set of folia `SegmentView`.
#[derive(Clone, Debug)]
pub struct SegmentNode {
    /// folia `segmentIndex: number`.
    pub segment_index: usize,
    /// folia `displayText: string`.
    pub display_text: String,
    /// folia `role: SonnetSegmentRole`.
    pub role: SonnetSegmentRole,
    /// folia `fontScale: number`.
    pub font_scale: f64,
    /// folia `x: number`.
    pub x: f64,
    /// folia `y: number`.
    pub y: f64,
    /// folia `rotation: number`.
    pub rotation: f64,
    /// folia `enterX: number`.
    pub enter_x: f64,
    /// folia `enterY: number`.
    pub enter_y: f64,
    /// folia `vertical: boolean`.
    pub vertical: bool,
    /// folia `timingPhase: number`.
    pub timing_phase: f64,
    /// folia `guide: SonnetGuideView` — forward slot for Phase 6.2.
    pub guide_slot: SlotId,
    /// folia `frameDecor?: SonnetFrameDecorView | null` — forward slot.
    pub frame_decor_slot: Option<SlotId>,
    /// folia `glyphs: GlyphView[]` — arena indices into `glyphs`.
    pub glyph_indices: Vec<usize>,
    /// folia `trackingGlyphs: GlyphView[]` — arena indices into `glyphs`.
    pub tracking_glyph_indices: Vec<usize>,
}

// ============================================================================
// Shot node — faithful port of folia `ShotView`.
// ----------------------------------------------------------------------------
// The five PIXI layers (`haloLayer`, `mgLayer`, `mgBackgroundLayer?`,
// `mgGeoLayer?`, `mgParticleLayer?`, `mgFixedGeoLayer?`) become `SlotId`s.
// `container: PIXI.Container` becomes `container_slot`.
// ============================================================================

/// Arena node carrying the exact field set of folia `ShotView`.
#[derive(Clone, Debug)]
pub struct ShotNode {
    /// folia `shot: SonnetShot` — owned by value (serialisable program contract).
    pub shot: SonnetShot,
    /// folia `container: PIXI.Container`.
    pub container_slot: SlotId,
    /// folia `segments: SegmentView[]` — arena indices into `segments`.
    pub segment_indices: Vec<usize>,
    /// folia `debugInfo: SonnetDebugShotInfo` — forward slot for Phase 6.2.
    pub debug_info_slot: SlotId,
    /// folia `baseX: number`.
    pub base_x: f64,
    /// folia `baseY: number`.
    pub base_y: f64,
    /// folia `basePivotX: number`.
    pub base_pivot_x: f64,
    /// folia `basePivotY: number`.
    pub base_pivot_y: f64,
    /// folia `haloLayer: PIXI.Container`.
    pub halo_layer_slot: SlotId,
    /// folia `mgLayer: PIXI.Container`.
    pub mg_layer_slot: SlotId,
    /// folia `mgBackgroundLayer?: PIXI.Container`.
    pub mg_background_layer_slot: Option<SlotId>,
    /// folia `mgGeoLayer?: PIXI.Container`.
    pub mg_geo_layer_slot: Option<SlotId>,
    /// folia `mgParticleLayer?: PIXI.Container`.
    pub mg_particle_layer_slot: Option<SlotId>,
    /// folia `mgFixedGeoLayer?: PIXI.Container`.
    pub mg_fixed_geo_layer_slot: Option<SlotId>,
}

// ============================================================================
// Scene node — faithful port of folia `SceneView`.
// ----------------------------------------------------------------------------
// `postProcessFilters: PIXI.Filter[]` becomes a `Vec<SlotId>` (one slot per
// filter, since folia attaches live WGSL/GLSL filter programs — Phase 7 maps
// these to the existing `draw.rs` WGSL `scene_at` post chain).
// `transitionBlurFilter: PIXI.BlurFilter | null` → `Option<SlotId>`.
// `transitionGlitchEffect: SonnetGlitchEffect | null` → forward slot.
// ============================================================================

/// Arena node carrying the exact field set of folia `SceneView`.
#[derive(Clone, Debug)]
pub struct SceneNode {
    /// folia `paragraph: SonnetParagraph`.
    pub paragraph: SonnetParagraph,
    /// folia `container: PIXI.Container`.
    pub container_slot: SlotId,
    /// folia `shots: ShotView[]` — arena indices into `shots`.
    pub shot_indices: Vec<usize>,
    /// folia `shotTimeline: SonnetShot[]` — owned by value.
    pub shot_timeline: Vec<SonnetShot>,
    /// folia `postProcessFilters: PIXI.Filter[]` — one slot per filter.
    pub post_process_filter_slots: Vec<SlotId>,
    /// folia `transitionBlurFilter: PIXI.BlurFilter | null`.
    pub transition_blur_filter_slot: Option<SlotId>,
    /// folia `transitionGlitchEffect: SonnetGlitchEffect | null` — forward slot.
    pub transition_glitch_slot: Option<SlotId>,
    /// folia `activeShotIndex: number`.
    pub active_shot_index: usize,
}

// ============================================================================
// SceneArena — the persistent graph the runtime mutates each frame.
// ----------------------------------------------------------------------------
// Folia's `sceneCache: Map<paragraphIndex, SceneView>` becomes one arena that
// owns all scene/shot/segment/glyph nodes for all cached paragraphs in a flat
// vec layout. `SlotId` indexes are stable across `clear`/`reset`, mirroring
// how folia reuses scene objects across playhead traversal.
// ============================================================================

/// The persistent X-architecture scene arena. Owned by the runtime; mutated
/// each frame; flattened to `Vec<CharQuad>` at frame end for `draw.rs`.
#[derive(Default)]
pub struct SceneArena {
    /// All glyph nodes (across all cached paragraphs), flat-owned.
    pub glyphs: Vec<GlyphNode>,
    /// All segment nodes, flat-owned. Each holds glyph indices.
    pub segments: Vec<SegmentNode>,
    /// All shot nodes, flat-owned. Each holds segment indices.
    pub shots: Vec<ShotNode>,
    /// All scene nodes, flat-owned. Each holds shot indices.
    pub scenes: Vec<SceneNode>,
    /// Free-slot bookkeeping slot ids (Phase 6.2 `unload_scenes ±1` helper).
    pub free_glyph_slots: Vec<usize>,
    pub free_segment_slots: Vec<usize>,
    pub free_shot_slots: Vec<usize>,
}

impl SceneArena {
    /// Allocate a fresh glyph node and return its arena index.
    pub fn push_glyph(&mut self, node: GlyphNode) -> usize {
        if let Some(idx) = self.free_glyph_slots.pop() {
            self.glyphs[idx] = node;
            idx
        } else {
            self.glyphs.push(node);
            self.glyphs.len() - 1
        }
    }

    /// Allocate a fresh segment node and return its arena index.
    pub fn push_segment(&mut self, node: SegmentNode) -> usize {
        if let Some(idx) = self.free_segment_slots.pop() {
            self.segments[idx] = node;
            idx
        } else {
            self.segments.push(node);
            self.segments.len() - 1
        }
    }

    /// Allocate a fresh shot node and return its arena index.
    pub fn push_shot(&mut self, node: ShotNode) -> usize {
        if let Some(idx) = self.free_shot_slots.pop() {
            self.shots[idx] = node;
            idx
        } else {
            self.shots.push(node);
            self.shots.len() - 1
        }
    }

    /// Allocate a fresh scene node and return its arena index.
    pub fn push_scene(&mut self, node: SceneNode) -> usize {
        self.scenes.push(node);
        self.scenes.len() - 1
    }

    /// Borrow a glyph node by arena index (panics if out of range — caller bug).
    pub fn glyph(&self, idx: usize) -> &GlyphNode {
        &self.glyphs[idx]
    }
    /// Mutably borrow a glyph node.
    pub fn glyph_mut(&mut self, idx: usize) -> &mut GlyphNode {
        &mut self.glyphs[idx]
    }
    /// Borrow a segment node.
    pub fn segment(&self, idx: usize) -> &SegmentNode {
        &self.segments[idx]
    }
    /// Mutably borrow a segment node.
    pub fn segment_mut(&mut self, idx: usize) -> &mut SegmentNode {
        &mut self.segments[idx]
    }
    /// Borrow a shot node.
    pub fn shot(&self, idx: usize) -> &ShotNode {
        &self.shots[idx]
    }
    /// Mutably borrow a shot node.
    pub fn shot_mut(&mut self, idx: usize) -> &mut ShotNode {
        &mut self.shots[idx]
    }
    /// Borrow a scene node.
    pub fn scene(&self, idx: usize) -> &SceneNode {
        &self.scenes[idx]
    }
    /// Mutably borrow a scene node.
    pub fn scene_mut(&mut self, idx: usize) -> &mut SceneNode {
        &mut self.scenes[idx]
    }

    /// Mark a glyph slot free for reuse (Phase 6.2 parity with folia's
    /// `sceneCache.delete(paragraphIndex)` minus the `±1` neighbour keep).
    pub fn free_glyph(&mut self, idx: usize) {
        self.free_glyph_slots.push(idx);
    }
}

// ============================================================================
// flatten_scene — runtime→draw.rs bridge (Phase 6.1 stub).
// ----------------------------------------------------------------------------
// Folia's runtime mutates PIXI display objects in place each frame; the draw
// happens automatically when the PIXI application ticks. The Rust X-port
// replaces that with: mutate arena nodes per frame → `flatten_scene` walks
// the mutated graph and emits a `Vec<CharQuad>` for `draw.rs`'s scene upload.
//
// The full walk (resolve per-glyph alpha/scale/rotation at `time`, mirror
// folia `renderFrame`'s sequence of `view.container.*` mutations) lands in
// Phase 6.2. For Phase 6.1 we ship the empty-stub so Phase 6.2+ can attach
// the runtime without reshaping the arena types.
// ============================================================================

/// Flatten the mutated scene graph at `time` into a `CharQuad` list for
/// `draw.rs`. Phase 6.1 stub — returns empty; Phase 6.2 implements the
/// actual per-glyph emit walk mirroring folia `renderFrame`.
pub fn flatten_scene(_arena: &SceneArena, _scene_idx: usize, _time: f64) -> Vec<crate::lyricview::CharQuad> {
    Vec::new()
}

// ============================================================================
// Tests — verify arena slot allocation/reuse + SlotId null sentinel.
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_id_null_sentinel_matches_folia_null() {
        // folia uses `null` for absent display objects; the port reserves
        // `u32::MAX` as the equivalent null and the arena never hands it out
        // (indices are always < len, which is << u32::MAX for real songs).
        assert!(SlotId::NULL.is_null());
        assert_eq!(SlotId::NULL.0, u32::MAX);
        assert!(!SlotId(0).is_null());
        assert!(!SlotId(42).is_null());
    }

    #[test]
    fn push_glyph_returns_consecutive_indices() {
        let mut arena = SceneArena::default();
        let a = arena.push_glyph(GlyphNode::default());
        let b = arena.push_glyph(GlyphNode {
            base_x: 10.0,
            ..GlyphNode::default()
        });
        assert_eq!(a, 0);
        assert_eq!(b, 1);
        assert_eq!(arena.glyph(b).base_x, 10.0);
        assert_eq!(arena.glyph(a).base_x, 0.0);
    }

    #[test]
    fn free_then_push_reuses_slot_index() {
        let mut arena = SceneArena::default();
        let a = arena.push_glyph(GlyphNode::default());
        let b = arena.push_glyph(GlyphNode::default());
        arena.free_glyph(a);
        let c = arena.push_glyph(GlyphNode {
            base_y: 7.0,
            ..GlyphNode::default()
        });
        // reuses `a`'s freed slot, not `b`'s tail slot.
        assert_eq!(c, a);
        assert_ne!(c, b);
        assert_eq!(arena.glyph(c).base_y, 7.0);
    }

    #[test]
    fn segment_shot_scene_indexing_round_trip() {
        let mut arena = SceneArena::default();
        let seg0 = arena.push_segment(SegmentNode {
            segment_index: 0,
            display_text: "测试".into(),
            role: SonnetSegmentRole::Hero,
            font_scale: 1.0,
            x: 100.0,
            y: 200.0,
            rotation: 0.0,
            enter_x: 0.0,
            enter_y: 0.0,
            vertical: false,
            timing_phase: 0.0,
            guide_slot: SlotId::NULL,
            frame_decor_slot: None,
            glyph_indices: vec![],
            tracking_glyph_indices: vec![],
        });
        let shot0 = arena.push_shot(ShotNode {
            shot: SonnetShot {
                id: "s0".into(),
                kind: SonnetShotKind::FragmentCollage,
                start_time: 0.0,
                end_time: 4.0,
                line_indices: vec![0],
                cues: vec![],
                camera: crate::lyricstyles::sonnet_v2::types::SonnetCameraFrame {
                    x: 0.0,
                    y: 0.0,
                    zoom: 1.0,
                    rotation: 0.0,
                },
            },
            container_slot: SlotId::NULL,
            segment_indices: vec![seg0],
            debug_info_slot: SlotId::NULL,
            base_x: 0.0,
            base_y: 0.0,
            base_pivot_x: 0.0,
            base_pivot_y: 0.0,
            halo_layer_slot: SlotId::NULL,
            mg_layer_slot: SlotId::NULL,
            mg_background_layer_slot: None,
            mg_geo_layer_slot: None,
            mg_particle_layer_slot: None,
            mg_fixed_geo_layer_slot: None,
        });
        let scene0 = arena.push_scene(SceneNode {
            paragraph: SonnetParagraph {
                id: "p0".into(),
                kind: crate::lyricstyles::sonnet_v2::types::SonnetParagraphKind::Verse,
                boundary: crate::lyricstyles::sonnet_v2::types::SonnetParagraphBoundary::SongStart,
                start_time: 0.0,
                end_time: 4.0,
                lines: vec![],
                shots: vec![],
                transition_out: None,
            },
            container_slot: SlotId::NULL,
            shot_indices: vec![shot0],
            shot_timeline: vec![],
            post_process_filter_slots: vec![],
            transition_blur_filter_slot: None,
            transition_glitch_slot: None,
            active_shot_index: 0,
        });
        assert_eq!(seg0, 0);
        assert_eq!(shot0, 0);
        assert_eq!(scene0, 0);
        assert_eq!(arena.scene(scene0).active_shot_index, 0);
        assert_eq!(arena.shot(shot0).segment_indices, vec![seg0]);
        assert_eq!(arena.segment(seg0).display_text, "测试");
    }

    #[test]
    fn flatten_scene_stub_returns_empty_for_phase_6_1() {
        let arena = SceneArena::default();
        let quads = flatten_scene(&arena, 0, 0.0);
        assert!(quads.is_empty(), "Phase 6.1 stub must return empty Vec");
    }
}
