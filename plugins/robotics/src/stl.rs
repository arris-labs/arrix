//! Binary STL from refined meshes: metres, as the snapshot gives them.

use arrix_plugin_api::Triangles;

const HEADER: &[u8] = b"ArriX robotics.urdf, binary STL, metres";

/// The triangles of every mesh in one file. A triangle that names a vertex
/// the mesh lacks is left out. Normals are the faces', from the winding.
pub fn binary(meshes: &[Triangles]) -> Vec<u8> {
    let mut body = Vec::new();
    let mut count = 0u32;
    for mesh in meshes {
        for tri in mesh.indices.as_chunks::<3>().0 {
            let vertex = |i: u32| {
                let at = i as usize * 3;
                mesh.positions.get(at..at + 3)
            };
            let (Some(a), Some(b), Some(c)) = (vertex(tri[0]), vertex(tri[1]), vertex(tri[2]))
            else {
                continue;
            };
            let n = normal(a, b, c);
            for v in [&n[..], a, b, c] {
                for x in v {
                    body.extend_from_slice(&x.to_le_bytes());
                }
            }
            body.extend_from_slice(&[0, 0]);
            count += 1;
        }
    }
    let mut out = Vec::with_capacity(84 + body.len());
    out.extend_from_slice(HEADER);
    out.resize(80, 0);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn normal(a: &[f32], b: &[f32], c: &[f32]) -> [f32; 3] {
    let d = |p: &[f32], q: &[f32]| [0, 1, 2].map(|i| f64::from(q[i]) - f64::from(p[i]));
    let (u, v) = (d(a, b), d(a, c));
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len > 0.0 {
        n.map(|x| (x / len) as f32)
    } else {
        [0.0; 3]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_triangle_is_134_bytes_with_its_normal() {
        let mesh = Triangles {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: vec![0.0; 9],
            indices: vec![0, 1, 2, 0, 1, 7],
        };
        let stl = binary(&[mesh]);
        assert_eq!(
            stl.len(),
            84 + 50,
            "the triangle with a missing vertex is left out"
        );
        assert_eq!(stl[80..84], 1u32.to_le_bytes());
        assert_eq!(
            &stl[84..96],
            [0.0f32, 0.0, 1.0].map(f32::to_le_bytes).concat()
        );
    }
}
