//! Load an original map from a glTF 2.0 file (`.gltf` JSON or `.glb` binary) into the same
//! [`MapDef`] the TOML maps use. Only the scene graph is read: node names, transforms and
//! the `POSITION` bounds (`min`/`max`, required by the glTF spec) of each mesh. Vertex data,
//! images and external buffers are never loaded, so this needs no filesystem access.
//!
//! Naming convention (see `docs/maps.md`):
//!
//! - Mesh nodes are solid boxes named by kind: `wall`, `cover` or `low_cover`, optionally
//!   followed by `.` or `:` and anything (`wall.001`, `cover:pillar`). Each becomes the
//!   world-space axis-aligned bounding box of its mesh. One or more meshes named `floor`
//!   set the walkable floor extent on X and Z. Any other mesh name is an error.
//! - Empties (nodes without a mesh) are markers named `<kind>:<id>`: `player:<id>` and
//!   `bot:<id>` are spawns facing the node's local −Z axis; `angle:<id>` is an angle
//!   marker's peek position and `angle_hidden:<id>` its hidden spot. Marker heights are
//!   ignored: feet stand on the floor (`y = 0`). Empties without `:` (groups, cameras,
//!   lights) are ignored; an unknown `<kind>:` is an error.
//!
//! glTF uses the same axes as the core: right-handed, +Y up, meters.

use crate::map::{AngleMarker, BoxKind, MapDef, SolidBox, Spawn, SpawnKind};
use aim_core::math::{Range, Vec3};
use aim_core::scenario::ValidationError;

#[derive(Debug, thiserror::Error)]
pub enum GltfMapError {
    #[error("invalid glTF: {0}")]
    Parse(#[from] gltf::Error),
    #[error("glTF map does not follow the naming convention: {}", .0.join("; "))]
    Convention(Vec<String>),
    #[error("invalid map: {}", join(.0))]
    Invalid(Vec<ValidationError>),
}

fn join(errs: &[ValidationError]) -> String {
    errs.iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ")
}

/// Column-major 4x4 matrix, as glTF stores it: `m[column][row]`.
type Mat4 = [[f32; 4]; 4];

const IDENTITY: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (c, col) in out.iter_mut().enumerate() {
        for (r, cell) in col.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

/// `m * [v, w]`, dropping the fourth component.
fn apply(m: &Mat4, v: [f32; 3], w: f32) -> Vec3 {
    let p = [v[0], v[1], v[2], w];
    let row = |r: usize| (0..4).map(|k| m[k][r] * p[k]).sum::<f32>();
    Vec3::new(row(0), row(1), row(2))
}

/// World-space axis-aligned bounds of the local box `[min, max]` under `m`.
fn world_bounds(m: &Mat4, min: [f32; 3], max: [f32; 3]) -> (Vec3, Vec3) {
    let mut lo = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut hi = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    for i in 0..8 {
        let corner = [
            if i & 1 == 0 { min[0] } else { max[0] },
            if i & 2 == 0 { min[1] } else { max[1] },
            if i & 4 == 0 { min[2] } else { max[2] },
        ];
        let p = apply(m, corner, 1.0);
        lo = Vec3::new(lo.x.min(p.x), lo.y.min(p.y), lo.z.min(p.z));
        hi = Vec3::new(hi.x.max(p.x), hi.y.max(p.y), hi.z.max(p.z));
    }
    (lo, hi)
}

fn vec3_of(value: Option<gltf::json::Value>) -> Option<[f32; 3]> {
    let value = value?;
    let a = value.as_array()?;
    if a.len() != 3 {
        return None;
    }
    let n = |i: usize| a[i].as_f64().map(|f| f as f32);
    Some([n(0)?, n(1)?, n(2)?])
}

/// Local bounds of every primitive of a mesh, merged.
fn mesh_bounds(mesh: &gltf::Mesh) -> Option<([f32; 3], [f32; 3])> {
    let mut out: Option<([f32; 3], [f32; 3])> = None;
    for prim in mesh.primitives() {
        let acc = prim.get(&gltf::Semantic::Positions)?;
        let (lo, hi) = (vec3_of(acc.min())?, vec3_of(acc.max())?);
        out = Some(match out {
            None => (lo, hi),
            Some((a, b)) => (
                [a[0].min(lo[0]), a[1].min(lo[1]), a[2].min(lo[2])],
                [b[0].max(hi[0]), b[1].max(hi[1]), b[2].max(hi[2])],
            ),
        });
    }
    out
}

#[derive(Default)]
struct Collected {
    floor: Option<(Vec3, Vec3)>,
    boxes: Vec<SolidBox>,
    spawns: Vec<Spawn>,
    peeks: Vec<(String, Vec3)>,
    hiddens: Vec<(String, Vec3)>,
    problems: Vec<String>,
}

fn on_floor(p: Vec3) -> Vec3 {
    Vec3::new(p.x, 0.0, p.z)
}

/// Deepest node nesting accepted. Real maps are a few levels deep; the limit keeps a
/// malformed or hostile file from overflowing the stack.
pub const MAX_NODE_DEPTH: usize = 64;

impl Collected {
    fn visit(&mut self, node: gltf::Node, parent: &Mat4, depth: usize) {
        if depth > MAX_NODE_DEPTH {
            self.problems.push(format!(
                "node '{}' is nested deeper than {MAX_NODE_DEPTH} levels",
                node.name().unwrap_or("")
            ));
            return;
        }
        let world = mul(parent, &node.transform().matrix());
        let name = node.name().unwrap_or("");
        match node.mesh() {
            Some(mesh) => self.mesh(name, &mesh, &world),
            None => self.marker(name, &world),
        }
        for child in node.children() {
            self.visit(child, &world, depth + 1);
        }
    }

    fn mesh(&mut self, name: &str, mesh: &gltf::Mesh, world: &Mat4) {
        let kind = name.split(['.', ':']).next().unwrap_or("");
        let Some((min, max)) = mesh_bounds(mesh) else {
            self.problems.push(format!(
                "mesh node '{name}' has a primitive without POSITION min/max bounds"
            ));
            return;
        };
        let (lo, hi) = world_bounds(world, min, max);
        let kind = match kind {
            "floor" => {
                self.floor = Some(match self.floor {
                    None => (lo, hi),
                    Some((a, b)) => (
                        Vec3::new(a.x.min(lo.x), 0.0, a.z.min(lo.z)),
                        Vec3::new(b.x.max(hi.x), 0.0, b.z.max(hi.z)),
                    ),
                });
                return;
            }
            "wall" => BoxKind::Wall,
            "cover" => BoxKind::Cover,
            "low_cover" => BoxKind::LowCover,
            _ => {
                self.problems.push(format!(
                    "mesh node '{name}' must be named floor, wall, cover or low_cover"
                ));
                return;
            }
        };
        self.boxes.push(SolidBox {
            kind,
            min: lo,
            max: hi,
        });
    }

    fn marker(&mut self, name: &str, world: &Mat4) {
        let Some((kind, id)) = name.split_once(':') else {
            return;
        };
        let at = on_floor(apply(world, [0.0; 3], 1.0));
        let id = id.to_owned();
        match kind {
            "player" | "bot" => {
                let f = apply(world, [0.0, 0.0, -1.0], 0.0);
                let yaw_deg = if f.x == 0.0 && f.z == 0.0 {
                    0.0
                } else {
                    f.x.atan2(-f.z).to_degrees()
                };
                self.spawns.push(Spawn {
                    id,
                    kind: if kind == "player" {
                        SpawnKind::Player
                    } else {
                        SpawnKind::Bot
                    },
                    position: at,
                    yaw_deg,
                });
            }
            "angle" => self.peeks.push((id, at)),
            "angle_hidden" => self.hiddens.push((id, at)),
            _ => self.problems.push(format!(
                "marker '{name}': unknown kind '{kind}' (player, bot, angle or angle_hidden)"
            )),
        }
    }
}

impl MapDef {
    /// Build and validate a map from the bytes of a `.gltf` (JSON with embedded or no
    /// buffers) or `.glb` file, following the naming convention in [`crate::gltf`]. The
    /// map gets `id` and `name`; it has no waypoint graph (bots walk straight lines).
    pub fn from_gltf(bytes: &[u8], id: &str, name: &str) -> Result<Self, GltfMapError> {
        let doc = gltf::Gltf::from_slice(bytes)?.document;
        let mut c = Collected::default();
        match doc.default_scene().or_else(|| doc.scenes().next()) {
            Some(scene) => {
                for node in scene.nodes() {
                    c.visit(node, &IDENTITY, 0);
                }
            }
            None => c.problems.push("the file has no scene".into()),
        }
        let floor = c.floor.unwrap_or_else(|| {
            c.problems.push("no mesh node named 'floor'".into());
            (Vec3::ZERO, Vec3::ZERO)
        });
        let mut angles = Vec::new();
        for (id, position) in &c.peeks {
            let Some((_, hidden)) = c.hiddens.iter().find(|(h, _)| h == id) else {
                c.problems
                    .push(format!("angle '{id}' has no 'angle_hidden:{id}' marker"));
                continue;
            };
            let d = *position - *hidden;
            angles.push(AngleMarker {
                id: id.clone(),
                label: id.clone(),
                position: *position,
                peek_dir: [d.x, d.z],
                peek_distance: d.horizontal().length(),
            });
        }
        for (id, _) in &c.hiddens {
            if !c.peeks.iter().any(|(p, _)| p == id) {
                c.problems
                    .push(format!("'angle_hidden:{id}' has no 'angle:{id}' marker"));
            }
        }
        if !c.problems.is_empty() {
            return Err(GltfMapError::Convention(c.problems));
        }
        let map = MapDef {
            id: id.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            floor_x: Range::new(floor.0.x, floor.1.x),
            floor_z: Range::new(floor.0.z, floor.1.z),
            boxes: c.boxes,
            spawns: c.spawns,
            angles,
            waypoints: Vec::new(),
        };
        let errs = map.validate();
        if errs.is_empty() {
            Ok(map)
        } else {
            Err(GltfMapError::Invalid(errs))
        }
    }
}
