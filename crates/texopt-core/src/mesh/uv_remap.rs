//! Pure UV math for moving a material's texture into an atlas rectangle.
//!
//! # Conventions
//! * Atlas rectangles are in **image pixels, origin top-left, y down** (what
//!   the packer produces and what image files store).
//! * Assimp delivers UVs exactly as stored in FBX/OBJ/DAE: **origin
//!   bottom-left, v up** (OpenGL convention; Unity, Godot and Blender use the
//!   same). Use [`UvOrigin::BottomLeft`] for meshes coming from
//!   [`crate::mesh::import`]. [`UvOrigin::TopLeft`] is for data that was
//!   flipped already (Assimp `aiProcess_FlipUVs`, DirectX/Unreal-style UVs).
//! * Remap: `uv' = offset + normalize(uv) * scale`, where `normalize` deals
//!   with UVs outside `[0,1]` according to [`OutOfRangePolicy`].
//! * The packer must not rotate rectangles used for meshes.

use serde::{Deserialize, Serialize};

use crate::mesh::codes;
use crate::{OpError, OpResult};

/// Tolerance used for "is this UV outside [0,1]" and tile decisions, so float
/// noise such as `1.0000001` from DCC exporters is not treated as tiling.
pub const UV_EPSILON: f32 = 1e-4;

/// Placement of one source texture inside the atlas, in pixels (top-left
/// origin). Same layout for every channel (albedo/normal/...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UvOrigin {
    /// v = 0 is the bottom edge of the image (Assimp/OpenGL/Unity/Godot).
    #[default]
    BottomLeft,
    /// v = 0 is the top edge of the image (DirectX/Unreal, FlipUVs).
    TopLeft,
}

/// How far inside the rectangle UV 0 and 1 land.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum InsetPolicy {
    /// UV 0/1 map exactly onto the rectangle edges.
    None,
    /// UV 0/1 map onto the centers of the edge texels so bilinear filtering
    /// never reads the neighbour sprite (scales the texture by `(w-1)/w`).
    #[default]
    HalfTexel,
    /// Custom inset in pixels on every side.
    Pixels { pixels: f32 },
}

impl InsetPolicy {
    pub fn pixels(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::HalfTexel => 0.5,
            Self::Pixels { pixels } => pixels,
        }
    }
}

/// `uv' = offset + uv * scale` (component-wise).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UvTransform {
    pub offset: [f32; 2],
    pub scale: [f32; 2],
}

impl UvTransform {
    pub const IDENTITY: Self = Self {
        offset: [0.0, 0.0],
        scale: [1.0, 1.0],
    };

    pub fn apply(&self, uv: [f32; 2]) -> [f32; 2] {
        [
            self.offset[0] + uv[0] * self.scale[0],
            self.offset[1] + uv[1] * self.scale[1],
        ]
    }
}

/// Transform that moves the unit UV square onto `rect` of an atlas of
/// `atlas_size` = `[width, height]` pixels.
pub fn atlas_transform(
    rect: AtlasRect,
    atlas_size: [u32; 2],
    inset: InsetPolicy,
    origin: UvOrigin,
) -> OpResult<UvTransform> {
    let [aw, ah] = atlas_size;
    if aw == 0 || ah == 0 {
        return Err(OpError::invalid_param("atlasSize", "empty"));
    }
    if rect.width == 0 || rect.height == 0 {
        return Err(OpError::invalid_param("rect", "empty"));
    }
    if u64::from(rect.x) + u64::from(rect.width) > u64::from(aw)
        || u64::from(rect.y) + u64::from(rect.height) > u64::from(ah)
    {
        return Err(OpError::invalid_param("rect", "outOfBounds"));
    }
    let p = f64::from(inset.pixels());
    if !p.is_finite()
        || p < 0.0
        || 2.0 * p >= f64::from(rect.width)
        || 2.0 * p >= f64::from(rect.height)
    {
        return Err(OpError::invalid_param("inset", "tooLarge"));
    }
    let (w, h) = (f64::from(aw), f64::from(ah));
    let x0 = f64::from(rect.x) + p;
    let x1 = f64::from(rect.x) + f64::from(rect.width) - p;
    let top = f64::from(rect.y) + p;
    let bottom = f64::from(rect.y) + f64::from(rect.height) - p;
    let offset_v = match origin {
        UvOrigin::TopLeft => top / h,
        UvOrigin::BottomLeft => 1.0 - bottom / h,
    };
    Ok(UvTransform {
        offset: [(x0 / w) as f32, offset_v as f32],
        scale: [((x1 - x0) / w) as f32, ((bottom - top) / h) as f32],
    })
}

/// Continuous pixel position (x right, y down) of `uv` in an image of `size`.
/// Sample with `floor()` for nearest-neighbour.
pub fn uv_to_pixel(uv: [f32; 2], size: [u32; 2], origin: UvOrigin) -> [f32; 2] {
    let x = uv[0] * size[0] as f32;
    let y = match origin {
        UvOrigin::TopLeft => uv[1] * size[1] as f32,
        UvOrigin::BottomLeft => (1.0 - uv[1]) * size[1] as f32,
    };
    [x, y]
}

/// Bounding box of a set of UVs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UvRangeReport {
    pub min: [f32; 2],
    pub max: [f32; 2],
    /// Some UV lies outside `[0,1]` by more than [`UV_EPSILON`] (tiling).
    pub out_of_range: bool,
}

impl UvRangeReport {
    pub fn merge(self, other: Self) -> Self {
        Self {
            min: [self.min[0].min(other.min[0]), self.min[1].min(other.min[1])],
            max: [self.max[0].max(other.max[0]), self.max[1].max(other.max[1])],
            out_of_range: self.out_of_range || other.out_of_range,
        }
    }
}

/// `None` for an empty set.
pub fn uv_range<'a>(uvs: impl IntoIterator<Item = &'a [f32; 2]>) -> Option<UvRangeReport> {
    let mut it = uvs.into_iter();
    let first = *it.next()?;
    let (mut min, mut max) = (first, first);
    for uv in it {
        for a in 0..2 {
            min[a] = min[a].min(uv[a]);
            max[a] = max[a].max(uv[a]);
        }
    }
    let out_of_range =
        min.iter().any(|&m| m < -UV_EPSILON) || max.iter().any(|&m| m > 1.0 + UV_EPSILON);
    Some(UvRangeReport {
        min,
        max,
        out_of_range,
    })
}

/// What to do with a material whose UVs leave `[0,1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum OutOfRangePolicy {
    /// Warn and keep the material out of the atlas (textures/UVs untouched).
    #[default]
    SkipMaterial,
    /// Clamp UVs into `[0,1]` (only sensible for tiny overshoots).
    Clamp,
    /// Move every face back into the unit tile (subtract its tile index).
    /// Only correct when no face straddles a tile border; straddling faces and
    /// shared vertices with conflicting tiles are counted and reported.
    WrapIntoTile,
    /// The packer stores the texture repeated `tiles[0] x tiles[1]` times and
    /// UVs are squeezed into that block. Fails if more than `max_tiles` per axis
    /// are required.
    BakeRepeat { max_tiles: u32 },
}

/// Per-vertex normalisation applied before the atlas transform.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UvNormalize {
    #[default]
    None,
    Clamp,
    Wrap,
    /// `uv_n = (uv - origin) / tiles`.
    Repeat {
        origin: [i32; 2],
        tiles: [u32; 2],
    },
}

/// Decision for one material.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum MaterialPlan {
    /// Leave the material out of the atlas.
    Skip,
    /// Atlas the material; for `Repeat` the packer must provide the texture
    /// repeated `tiles` times (see [`MaterialPlan::repeat_tiles`]).
    Remap { normalize: UvNormalize },
}

impl MaterialPlan {
    /// How many copies of the texture (u, v) the atlas rectangle must hold.
    pub fn repeat_tiles(&self) -> [u32; 2] {
        match self {
            Self::Remap {
                normalize: UvNormalize::Repeat { tiles, .. },
            } => *tiles,
            _ => [1, 1],
        }
    }
}

/// Integer tile block covering the range: `(origin, tiles)`.
pub fn repeat_tiles(report: &UvRangeReport) -> ([i32; 2], [u32; 2]) {
    let mut origin = [0i32; 2];
    let mut tiles = [1u32; 2];
    for a in 0..2 {
        let start = (report.min[a] + UV_EPSILON).floor();
        let end = (report.max[a] - UV_EPSILON).ceil();
        origin[a] = start as i32;
        tiles[a] = ((end - start) as i64).max(1) as u32;
    }
    (origin, tiles)
}

/// Decide how to treat a material given its UV range and the user policy.
pub fn plan_material(report: &UvRangeReport, policy: OutOfRangePolicy) -> OpResult<MaterialPlan> {
    if !report.out_of_range {
        return Ok(MaterialPlan::Remap {
            normalize: UvNormalize::None,
        });
    }
    Ok(match policy {
        OutOfRangePolicy::SkipMaterial => MaterialPlan::Skip,
        OutOfRangePolicy::Clamp => MaterialPlan::Remap {
            normalize: UvNormalize::Clamp,
        },
        OutOfRangePolicy::WrapIntoTile => MaterialPlan::Remap {
            normalize: UvNormalize::Wrap,
        },
        OutOfRangePolicy::BakeRepeat { max_tiles } => {
            let (origin, tiles) = repeat_tiles(report);
            if tiles[0] > max_tiles || tiles[1] > max_tiles {
                return Err(OpError::new(codes::MESH_UV_TOO_MANY_TILES)
                    .with("tilesU", tiles[0])
                    .with("tilesV", tiles[1])
                    .with("maxTiles", max_tiles));
            }
            MaterialPlan::Remap {
                normalize: UvNormalize::Repeat { origin, tiles },
            }
        }
    })
}

/// Result of [`wrap_into_tile`].
#[derive(Debug, Clone, PartialEq)]
pub struct WrapResult {
    pub uvs: Vec<[f32; 2]>,
    /// Faces spanning more than one tile (texture seams will be wrong).
    pub straddling_faces: usize,
    /// Vertices shared by faces that live in different tiles (the vertex keeps
    /// the first face's tile; the other faces will be distorted).
    pub conflicting_vertices: usize,
}

fn tile_of(v: f32) -> f32 {
    (v + UV_EPSILON).floor()
}

/// Shift each face into the unit tile by subtracting `floor(min corner)` of
/// that face. Unlike a per-vertex `fract()`, a face spanning e.g. `u ∈ [1,2]`
/// keeps its right edge at `1.0` instead of collapsing it to `0.0`.
pub fn wrap_into_tile(uvs: &[[f32; 2]], faces: &[Vec<u32>]) -> WrapResult {
    let mut tile: Vec<Option<[f32; 2]>> = vec![None; uvs.len()];
    let mut conflicted = vec![false; uvs.len()];
    let mut straddling_faces = 0;
    for face in faces {
        let idx: Vec<usize> = face
            .iter()
            .map(|&i| i as usize)
            .filter(|&i| i < uvs.len())
            .collect();
        if idx.is_empty() {
            continue;
        }
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        for &i in &idx {
            for a in 0..2 {
                min[a] = min[a].min(uvs[i][a]);
                max[a] = max[a].max(uvs[i][a]);
            }
        }
        let t = [tile_of(min[0]), tile_of(min[1])];
        if (0..2).any(|a| max[a] - t[a] > 1.0 + UV_EPSILON) {
            straddling_faces += 1;
        }
        for &i in &idx {
            match tile[i] {
                None => tile[i] = Some(t),
                Some(prev) if prev != t => conflicted[i] = true,
                Some(_) => {}
            }
        }
    }
    let out = uvs
        .iter()
        .zip(&tile)
        .map(|(uv, t)| {
            let t = t.unwrap_or([tile_of(uv[0]), tile_of(uv[1])]);
            [uv[0] - t[0], uv[1] - t[1]]
        })
        .collect();
    WrapResult {
        uvs: out,
        straddling_faces,
        conflicting_vertices: conflicted.iter().filter(|&&c| c).count(),
    }
}

/// Full per-material remap: normalisation followed by the atlas transform.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UvRemap {
    pub normalize: UvNormalize,
    pub transform: UvTransform,
}

/// Output of [`UvRemap::apply_mesh`].
#[derive(Debug, Clone, PartialEq)]
pub struct RemapOutput {
    pub uvs: Vec<[f32; 2]>,
    pub straddling_faces: usize,
    pub conflicting_vertices: usize,
}

impl UvRemap {
    pub fn new(normalize: UvNormalize, transform: UvTransform) -> Self {
        Self {
            normalize,
            transform,
        }
    }

    /// Remap a single UV. `Wrap` falls back to a per-vertex tile here; prefer
    /// [`UvRemap::apply_mesh`] which decides tiles per face.
    pub fn apply(&self, uv: [f32; 2]) -> [f32; 2] {
        let n = match self.normalize {
            UvNormalize::None => uv,
            UvNormalize::Clamp => [uv[0].clamp(0.0, 1.0), uv[1].clamp(0.0, 1.0)],
            UvNormalize::Wrap => [uv[0] - tile_of(uv[0]), uv[1] - tile_of(uv[1])],
            UvNormalize::Repeat { origin, tiles } => [
                (uv[0] - origin[0] as f32) / tiles[0] as f32,
                (uv[1] - origin[1] as f32) / tiles[1] as f32,
            ],
        };
        self.transform.apply(n)
    }

    /// Remap all UVs of a mesh (faces are needed for `Wrap`).
    pub fn apply_mesh(&self, uvs: &[[f32; 2]], faces: &[Vec<u32>]) -> RemapOutput {
        if self.normalize == UvNormalize::Wrap {
            let w = wrap_into_tile(uvs, faces);
            return RemapOutput {
                uvs: w
                    .uvs
                    .into_iter()
                    .map(|uv| self.transform.apply(uv))
                    .collect(),
                straddling_faces: w.straddling_faces,
                conflicting_vertices: w.conflicting_vertices,
            };
        }
        RemapOutput {
            uvs: uvs.iter().map(|&uv| self.apply(uv)).collect(),
            straddling_faces: 0,
            conflicting_vertices: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    fn close(a: [f32; 2], b: [f32; 2]) -> bool {
        (a[0] - b[0]).abs() < EPS && (a[1] - b[1]).abs() < EPS
    }

    fn rect(x: u32, y: u32, width: u32, height: u32) -> AtlasRect {
        AtlasRect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn identity_rect_is_identity_in_both_conventions() {
        for origin in [UvOrigin::BottomLeft, UvOrigin::TopLeft] {
            let t = atlas_transform(rect(0, 0, 512, 256), [512, 256], InsetPolicy::None, origin)
                .unwrap();
            assert_eq!(t, UvTransform::IDENTITY);
        }
    }

    #[test]
    fn quarter_rect_top_right_of_image() {
        // Top-right quadrant of the image = top-right quadrant of bottom-left UV space.
        let t = atlas_transform(
            rect(128, 0, 128, 128),
            [256, 256],
            InsetPolicy::None,
            UvOrigin::BottomLeft,
        )
        .unwrap();
        assert!(close(t.offset, [0.5, 0.5]), "{t:?}");
        assert!(close(t.scale, [0.5, 0.5]));
        assert!(close(t.apply([0.0, 0.0]), [0.5, 0.5]));
        assert!(close(t.apply([1.0, 1.0]), [1.0, 1.0]));
        // Same rect for top-left UVs: v starts at the image top.
        let t = atlas_transform(
            rect(128, 0, 128, 128),
            [256, 256],
            InsetPolicy::None,
            UvOrigin::TopLeft,
        )
        .unwrap();
        assert!(close(t.offset, [0.5, 0.0]));
        assert!(close(t.scale, [0.5, 0.5]));
    }

    #[test]
    fn quarter_rect_bottom_left_of_image() {
        let t = atlas_transform(
            rect(0, 128, 128, 128),
            [256, 256],
            InsetPolicy::None,
            UvOrigin::BottomLeft,
        )
        .unwrap();
        assert!(close(t.offset, [0.0, 0.0]));
        assert!(close(t.scale, [0.5, 0.5]));
        let t = atlas_transform(
            rect(0, 128, 128, 128),
            [256, 256],
            InsetPolicy::None,
            UvOrigin::TopLeft,
        )
        .unwrap();
        assert!(close(t.offset, [0.0, 0.5]));
    }

    #[test]
    fn half_texel_inset_values() {
        let t = atlas_transform(
            rect(0, 0, 64, 64),
            [256, 256],
            InsetPolicy::HalfTexel,
            UvOrigin::BottomLeft,
        )
        .unwrap();
        assert!(close(t.offset, [0.5 / 256.0, 1.0 - 63.5 / 256.0]), "{t:?}");
        assert!(close(t.scale, [63.0 / 256.0, 63.0 / 256.0]));
        // UV 0 and 1 hit the centres of the edge texels of the rect.
        let p0 = uv_to_pixel(t.apply([0.0, 1.0]), [256, 256], UvOrigin::BottomLeft);
        let p1 = uv_to_pixel(t.apply([1.0, 0.0]), [256, 256], UvOrigin::BottomLeft);
        assert!(close(p0, [0.5, 0.5]), "{p0:?}");
        assert!(close(p1, [63.5, 63.5]), "{p1:?}");
        let t = atlas_transform(
            rect(0, 0, 64, 64),
            [256, 256],
            InsetPolicy::HalfTexel,
            UvOrigin::TopLeft,
        )
        .unwrap();
        assert!(close(t.offset, [0.5 / 256.0, 0.5 / 256.0]));
    }

    #[test]
    fn custom_pixel_inset_and_validation() {
        let t = atlas_transform(
            rect(10, 20, 30, 40),
            [100, 100],
            InsetPolicy::Pixels { pixels: 2.0 },
            UvOrigin::TopLeft,
        )
        .unwrap();
        assert!(close(t.offset, [0.12, 0.22]));
        assert!(close(t.scale, [0.26, 0.36]));
        let err = atlas_transform(
            rect(0, 0, 4, 4),
            [8, 8],
            InsetPolicy::Pixels { pixels: 2.0 },
            UvOrigin::TopLeft,
        )
        .unwrap_err();
        assert_eq!(err.code, crate::error::codes::INVALID_PARAMS);
        let err = atlas_transform(
            rect(5, 0, 4, 4),
            [8, 8],
            InsetPolicy::None,
            UvOrigin::TopLeft,
        )
        .unwrap_err();
        assert_eq!(err.params["reason"], "outOfBounds");
        assert!(
            atlas_transform(
                rect(0, 0, 0, 4),
                [8, 8],
                InsetPolicy::None,
                UvOrigin::TopLeft
            )
            .is_err()
        );
        assert!(
            atlas_transform(
                rect(0, 0, 1, 1),
                [0, 8],
                InsetPolicy::None,
                UvOrigin::TopLeft
            )
            .is_err()
        );
    }

    #[test]
    fn v_flip_conventions_point_to_the_same_texel() {
        // A UV in bottom-left space and its flipped twin in top-left space must
        // land on the same atlas pixel.
        let r = rect(32, 64, 64, 32);
        let bl =
            atlas_transform(r, [128, 128], InsetPolicy::HalfTexel, UvOrigin::BottomLeft).unwrap();
        let tl = atlas_transform(r, [128, 128], InsetPolicy::HalfTexel, UvOrigin::TopLeft).unwrap();
        for uv in [[0.0, 0.0], [0.25, 0.75], [1.0, 1.0], [0.6, 0.1]] {
            let a = uv_to_pixel(bl.apply(uv), [128, 128], UvOrigin::BottomLeft);
            let b = uv_to_pixel(
                tl.apply([uv[0], 1.0 - uv[1]]),
                [128, 128],
                UvOrigin::TopLeft,
            );
            assert!(close(a, b), "{uv:?}: {a:?} vs {b:?}");
            assert!(a[0] >= 32.0 && a[0] <= 96.0 && a[1] >= 64.0 && a[1] <= 96.0);
        }
        // uv (0,0) bottom-left convention = bottom-left corner of the rect in the image.
        let p = uv_to_pixel(bl.apply([0.0, 0.0]), [128, 128], UvOrigin::BottomLeft);
        assert!(close(p, [32.5, 95.5]), "{p:?}");
    }

    #[test]
    fn range_detection() {
        let r = uv_range(&[[0.0, 0.0], [1.0, 1.0], [0.5, 1.00001]]).unwrap();
        assert!(!r.out_of_range, "float noise is not tiling");
        assert_eq!(r.min, [0.0, 0.0]);
        let r = uv_range(&[[0.0, 0.0], [2.0, 1.0]]).unwrap();
        assert!(r.out_of_range);
        assert_eq!(r.max, [2.0, 1.0]);
        let r = uv_range(&[[-0.5, 0.2]]).unwrap();
        assert!(r.out_of_range);
        assert!(uv_range(&[]).is_none());
        let merged = uv_range(&[[0.1, 0.1]])
            .unwrap()
            .merge(uv_range(&[[3.0, -1.0]]).unwrap());
        assert_eq!(merged.min, [0.1, -1.0]);
        assert_eq!(merged.max, [3.0, 0.1]);
        assert!(merged.out_of_range);
    }

    #[test]
    fn in_range_material_is_always_remapped_plainly() {
        let r = uv_range(&[[0.0, 0.0], [1.0, 1.0]]).unwrap();
        for p in [
            OutOfRangePolicy::SkipMaterial,
            OutOfRangePolicy::Clamp,
            OutOfRangePolicy::WrapIntoTile,
            OutOfRangePolicy::BakeRepeat { max_tiles: 1 },
        ] {
            assert_eq!(
                plan_material(&r, p).unwrap(),
                MaterialPlan::Remap {
                    normalize: UvNormalize::None
                }
            );
        }
    }

    #[test]
    fn policy_skip_and_clamp() {
        let r = uv_range(&[[0.0, 0.0], [2.0, 1.0]]).unwrap();
        assert_eq!(
            plan_material(&r, OutOfRangePolicy::SkipMaterial).unwrap(),
            MaterialPlan::Skip
        );
        let plan = plan_material(&r, OutOfRangePolicy::Clamp).unwrap();
        assert_eq!(
            plan,
            MaterialPlan::Remap {
                normalize: UvNormalize::Clamp
            }
        );
        let remap = UvRemap::new(UvNormalize::Clamp, UvTransform::IDENTITY);
        assert_eq!(remap.apply([2.0, -0.5]), [1.0, 0.0]);
        assert_eq!(remap.apply([0.3, 0.7]), [0.3, 0.7]);
    }

    #[test]
    fn policy_wrap_into_tile_per_face() {
        // Quad entirely in tile u=[1,2]: right edge must stay at 1.0, not fract() to 0.
        let uvs = vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]];
        let faces = vec![vec![0, 1, 2, 3]];
        let w = wrap_into_tile(&uvs, &faces);
        assert_eq!(w.uvs, vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        assert_eq!(w.straddling_faces, 0);
        assert_eq!(w.conflicting_vertices, 0);
        // Negative tile.
        let w = wrap_into_tile(&[[-1.0, -2.0], [-0.5, -1.5]], &[vec![0, 1]]);
        assert!(close(w.uvs[0], [0.0, 0.0]) && close(w.uvs[1], [0.5, 0.5]));
        let plan = plan_material(&uv_range(&uvs).unwrap(), OutOfRangePolicy::WrapIntoTile).unwrap();
        assert_eq!(
            plan,
            MaterialPlan::Remap {
                normalize: UvNormalize::Wrap
            }
        );
    }

    #[test]
    fn wrap_detects_straddling_faces_and_conflicting_vertices() {
        // Face 0 spans u ∈ [0.5, 1.5] → straddles. Face 1 in tile 1 shares vertex 1 with face 0 (tile 0).
        let uvs = vec![[0.5, 0.0], [1.5, 0.0], [1.5, 1.0], [1.9, 0.5]];
        let faces = vec![vec![0, 1, 2], vec![1, 3, 2]];
        let w = wrap_into_tile(&uvs, &faces);
        assert_eq!(w.straddling_faces, 1);
        assert_eq!(w.conflicting_vertices, 2); // vertices 1 and 2
        let remap = UvRemap::new(UvNormalize::Wrap, UvTransform::IDENTITY);
        let out = remap.apply_mesh(&uvs, &faces);
        assert_eq!(out.straddling_faces, 1);
        assert_eq!(out.conflicting_vertices, 2);
    }

    #[test]
    fn policy_bake_repeat_tile_counts() {
        let r = uv_range(&[[0.0, 0.0], [2.0, 1.0]]).unwrap();
        assert_eq!(repeat_tiles(&r), ([0, 0], [2, 1]));
        let r = uv_range(&[[-0.5, 0.0], [1.0, 3.2]]).unwrap();
        assert_eq!(repeat_tiles(&r), ([-1, 0], [2, 4]));
        let r = uv_range(&[[0.0, 0.0], [1.0000001, 1.0]]).unwrap();
        assert_eq!(repeat_tiles(&r), ([0, 0], [1, 1]));

        let r = uv_range(&[[0.0, 0.0], [3.0, 2.0]]).unwrap();
        let plan = plan_material(&r, OutOfRangePolicy::BakeRepeat { max_tiles: 4 }).unwrap();
        assert_eq!(
            plan,
            MaterialPlan::Remap {
                normalize: UvNormalize::Repeat {
                    origin: [0, 0],
                    tiles: [3, 2]
                }
            }
        );
        assert_eq!(plan.repeat_tiles(), [3, 2]);
        let remap = UvRemap::new(
            UvNormalize::Repeat {
                origin: [0, 0],
                tiles: [3, 2],
            },
            UvTransform::IDENTITY,
        );
        assert!(close(remap.apply([3.0, 2.0]), [1.0, 1.0]));
        assert!(close(remap.apply([1.5, 1.0]), [0.5, 0.5]));

        let err = plan_material(&r, OutOfRangePolicy::BakeRepeat { max_tiles: 2 }).unwrap_err();
        assert_eq!(err.code, codes::MESH_UV_TOO_MANY_TILES);
        assert_eq!(err.params["tilesU"], 3);
        assert_eq!(err.params["maxTiles"], 2);
    }

    #[test]
    fn remap_composes_normalize_then_transform() {
        let t = atlas_transform(
            rect(0, 0, 128, 128),
            [256, 256],
            InsetPolicy::None,
            UvOrigin::BottomLeft,
        )
        .unwrap();
        let remap = UvRemap::new(
            UvNormalize::Repeat {
                origin: [0, 0],
                tiles: [2, 1],
            },
            t,
        );
        // u = 2 → normalized 1 → atlas 0.5; v = 1 → top of the rect = 1.0 in BL space.
        assert!(close(remap.apply([2.0, 1.0]), [0.5, 1.0]));
        assert!(close(remap.apply([0.0, 0.0]), [0.0, 0.5]));
    }

    #[test]
    fn serde_shapes() {
        assert_eq!(
            serde_json::to_string(&InsetPolicy::HalfTexel).unwrap(),
            r#"{"mode":"halfTexel"}"#
        );
        assert_eq!(
            serde_json::to_string(&InsetPolicy::Pixels { pixels: 2.0 }).unwrap(),
            r#"{"mode":"pixels","pixels":2.0}"#
        );
        assert_eq!(
            serde_json::to_string(&OutOfRangePolicy::BakeRepeat { max_tiles: 4 }).unwrap(),
            r#"{"mode":"bakeRepeat","maxTiles":4}"#
        );
        assert_eq!(
            serde_json::to_string(&UvNormalize::Repeat {
                origin: [0, -1],
                tiles: [2, 3]
            })
            .unwrap(),
            r#"{"mode":"repeat","origin":[0,-1],"tiles":[2,3]}"#
        );
        let p: OutOfRangePolicy = serde_json::from_str(r#"{"mode":"wrapIntoTile"}"#).unwrap();
        assert_eq!(p, OutOfRangePolicy::WrapIntoTile);
    }
}
