//! A guest of the probe's scratch world, as one of its worlds by feature:
//!
//! - `full`: every export; `export` never calls `document`, so whether an
//!   unused import survives into the component can be seen;
//! - `exporter`: `exchange` alone, writing every part's mesh as one binary
//!   STL, so a large mesh crosses in and a large file crosses out;
//! - `features`: `feature` alone.

#[cfg(feature = "full")]
wit_bindgen::generate!({ path: "wit", world: "full" });
#[cfg(feature = "exporter")]
wit_bindgen::generate!({ path: "wit", world: "exporter" });
#[cfg(feature = "features")]
wit_bindgen::generate!({ path: "wit", world: "features" });

struct Probe;

#[cfg(any(feature = "full", feature = "features"))]
impl exports::arrix::probe::feature::Guest for Probe {
    fn describe() -> Vec<String> {
        vec!["probe.block".into()]
    }
}

#[cfg(feature = "full")]
impl exports::arrix::probe::exchange::Guest for Probe {
    fn exporters() -> Vec<String> {
        vec!["probe.empty".into()]
    }
    fn export(id: String) -> Result<Vec<exports::arrix::probe::exchange::File>, String> {
        Ok(vec![exports::arrix::probe::exchange::File {
            name: format!("{id}.txt"),
            bytes: b"no document read".to_vec(),
        }])
    }
}

#[cfg(feature = "exporter")]
impl exports::arrix::probe::exchange::Guest for Probe {
    fn exporters() -> Vec<String> {
        vec!["probe.stl".into()]
    }
    fn export(id: String) -> Result<Vec<exports::arrix::probe::exchange::File>, String> {
        use arrix::probe::document;
        if id != "probe.stl" {
            return Err(format!("no exporter {id}"));
        }
        let mut out = Vec::new();
        for part in document::parts() {
            let mesh = document::mesh(part);
            let mut bytes = vec![0u8; 80];
            let count = mesh.triangles.len() / 3;
            bytes.extend((count as u32).to_le_bytes());
            for t in mesh.triangles.chunks_exact(3) {
                let n = t[0] as usize * 3;
                for c in &mesh.normals[n..n + 3] {
                    bytes.extend(c.to_le_bytes());
                }
                for &v in t {
                    let p = v as usize * 3;
                    for c in &mesh.positions[p..p + 3] {
                        bytes.extend(c.to_le_bytes());
                    }
                }
                bytes.extend([0, 0]);
            }
            out.push(exports::arrix::probe::exchange::File {
                name: format!("meshes/{part}.stl"),
                bytes,
            });
        }
        Ok(out)
    }
}

export!(Probe);
