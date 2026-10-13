//! The URDF text and the files around it.

use std::fmt::Write as _;

use arrix_plugin_api::{
    ExportFile, Inertial, JointKind, MaterialInfo, PartId, Snapshot, Triangles,
};

use crate::model::{Link, Names, Robot};
use crate::rigid::Rigid;
use crate::stl;

/// What the exporter's options say.
pub struct Options {
    pub effort: f64,
    pub velocity: f64,
    /// `package://<robot>/` before a mesh's path.
    pub package: bool,
}

/// A prismatic joint with no limits still needs some in a URDF.
const UNLIMITED: f64 = 1000.0;

/// What a link shows and weighs, by part.
struct PartOut {
    part: PartId,
    mesh: Option<String>,
    material: Option<String>,
    inertial: Option<Inertial>,
}

pub fn files(mut robot: Robot, snapshot: &mut dyn Snapshot, opts: &Options) -> Vec<ExportFile> {
    let mut stls: Vec<ExportFile> = Vec::new();
    let mut materials: Vec<(String, MaterialInfo)> = Vec::new();
    let outs = part_outs(&mut robot, snapshot, &mut stls, &mut materials);

    let mut x = String::new();
    let _ = writeln!(x, "<?xml version=\"1.0\"?>");
    let _ = writeln!(
        x,
        "<!-- ArriX robotics.urdf: metres, kilograms, radians; z up. -->"
    );
    for w in &robot.warnings {
        let _ = writeln!(x, "<!-- warning: {} -->", comment(w));
    }
    let _ = writeln!(x, "<robot name=\"{}\">", robot.name);
    for (name, m) in &materials {
        let c = m.color;
        let _ = writeln!(
            x,
            "  <material name=\"{name}\"><color rgba=\"{} {} {} 1\"/></material>",
            num(c.red),
            num(c.green),
            num(c.blue)
        );
    }
    let _ = writeln!(x, "  <link name=\"base_link\"/>");
    for link in &robot.links {
        let out = outs
            .iter()
            .find(|o| o.part == link.part)
            .expect("every part is read");
        write_link(&mut x, link, out, &robot.name, opts);
    }
    let mut joint_names = Names::default();
    for link in &robot.links {
        write_joint(&mut x, &robot, link, &mut joint_names, opts);
    }
    let _ = writeln!(x, "</robot>");

    let main = ExportFile {
        name: format!("{}.urdf", robot.name),
        bytes: x.into_bytes(),
    };
    std::iter::once(main).chain(stls).collect()
}

/// Each used part's mesh file, material and mass, in order of first use.
fn part_outs(
    robot: &mut Robot,
    snapshot: &mut dyn Snapshot,
    stls: &mut Vec<ExportFile>,
    materials: &mut Vec<(String, MaterialInfo)>,
) -> Vec<PartOut> {
    let infos = snapshot.parts();
    let mut outs: Vec<PartOut> = Vec::new();
    let mut files = Names::default();
    let mut material_names = Names::default();
    for link in &robot.links {
        if outs.iter().any(|o| o.part == link.part) {
            continue;
        }
        let Some(info) = infos.iter().find(|p| p.id == link.part) else {
            robot
                .warnings
                .push(format!("part {} is not in the document", link.part));
            outs.push(PartOut {
                part: link.part,
                mesh: None,
                material: None,
                inertial: None,
            });
            continue;
        };
        let mut meshes: Vec<Triangles> = Vec::new();
        for b in &info.bodies {
            match snapshot.mesh(link.part, &b.body) {
                Ok(m) => meshes.push(m),
                Err(d) => robot.warnings.push(format!(
                    "{} body {}.{} has no mesh ({})",
                    info.name, b.body.feature, b.body.slot, d.code
                )),
            }
        }
        let mesh = (meshes.iter().any(|m| !m.indices.is_empty())).then(|| {
            let name = files.take(&info.name);
            stls.push(ExportFile {
                name: format!("meshes/{name}.stl"),
                bytes: stl::binary(&meshes),
            });
            name
        });
        let material = info
            .bodies
            .iter()
            .find_map(|b| b.material.as_ref())
            .map(|m| {
                let known = materials.iter().find(|(_, k)| k == m);
                match known {
                    Some((name, _)) => name.clone(),
                    None => {
                        let name = material_names.take(&m.name);
                        materials.push((name.clone(), m.clone()));
                        name
                    }
                }
            });
        let inertial = match snapshot.mass(link.part) {
            Ok(i) => Some(i),
            Err(d) => {
                robot.warnings.push(format!(
                    "{} has no mass ({}); its link has no inertial",
                    info.name, d.code
                ));
                None
            }
        };
        outs.push(PartOut {
            part: link.part,
            mesh,
            material,
            inertial,
        });
    }
    outs
}

fn write_link(x: &mut String, link: &Link, out: &PartOut, robot: &str, opts: &Options) {
    let _ = writeln!(x, "  <link name=\"{}\">", link.name);
    let at = link.part_in_link;
    if let Some(i) = &out.inertial {
        let c = at.apply([i.centroid.x, i.centroid.y, i.centroid.z]);
        // Products of inertia below the rounding of the diagonal are noise.
        let floor = 1e-9 * i.inertia.xx.max(i.inertia.yy).max(i.inertia.zz);
        let off = |x: f64| if x.abs() < floor { 0.0 } else { x };
        let mut t = i.inertia;
        (t.xy, t.xz, t.yz) = (off(t.xy), off(t.xz), off(t.yz));
        let _ = writeln!(x, "    <inertial>");
        let _ = writeln!(x, "      <origin {}/>", origin_at(c, &at));
        let _ = writeln!(x, "      <mass value=\"{}\"/>", sci(i.mass));
        let _ = writeln!(
            x,
            "      <inertia ixx=\"{}\" ixy=\"{}\" ixz=\"{}\" iyy=\"{}\" iyz=\"{}\" izz=\"{}\"/>",
            sci(t.xx),
            sci(t.xy),
            sci(t.xz),
            sci(t.yy),
            sci(t.yz),
            sci(t.zz)
        );
        let _ = writeln!(x, "    </inertial>");
    }
    if let Some(mesh) = &out.mesh {
        let path = if opts.package {
            format!("package://{robot}/meshes/{mesh}.stl")
        } else {
            format!("meshes/{mesh}.stl")
        };
        for tag in ["visual", "collision"] {
            let _ = writeln!(x, "    <{tag}>");
            let _ = writeln!(x, "      <origin {}/>", origin_at(at.t, &at));
            let _ = writeln!(x, "      <geometry><mesh filename=\"{path}\"/></geometry>");
            if tag == "visual"
                && let Some(m) = &out.material
            {
                let _ = writeln!(x, "      <material name=\"{m}\"/>");
            }
            let _ = writeln!(x, "    </{tag}>");
        }
    }
    let _ = writeln!(x, "  </link>");
}

fn write_joint(x: &mut String, robot: &Robot, link: &Link, names: &mut Names, opts: &Options) {
    let Some(joint) = &link.joint else {
        let name = names.take(&format!("base_to_{}", link.name));
        let _ = writeln!(x, "  <joint name=\"{name}\" type=\"fixed\">");
        let _ = writeln!(x, "    <parent link=\"base_link\"/>");
        let _ = writeln!(x, "    <child link=\"{}\"/>", link.name);
        let _ = writeln!(x, "    <origin {}/>", origin_at(link.frame.t, &link.frame));
        let _ = writeln!(x, "  </joint>");
        return;
    };
    let limits = joint.limits;
    let kind = match (joint.kind, limits) {
        (JointKind::Fixed, _) => "fixed",
        (JointKind::Revolute, Some(_)) => "revolute",
        (JointKind::Revolute, None) => "continuous",
        (JointKind::Prismatic, _) => "prismatic",
    };
    let name = names.take(&joint.name);
    let _ = writeln!(x, "  <joint name=\"{name}\" type=\"{kind}\">");
    let _ = writeln!(
        x,
        "    <parent link=\"{}\"/>",
        robot.links[joint.parent].name
    );
    let _ = writeln!(x, "    <child link=\"{}\"/>", link.name);
    let _ = writeln!(
        x,
        "    <origin {}/>",
        origin_at(joint.origin.t, &joint.origin)
    );
    if joint.kind != JointKind::Fixed {
        let _ = writeln!(x, "    <axis xyz=\"0 0 1\"/>");
        let effort_velocity = format!(
            "effort=\"{}\" velocity=\"{}\"",
            num(opts.effort),
            num(opts.velocity)
        );
        match (kind, limits) {
            ("continuous", _) => {
                let _ = writeln!(x, "    <limit {effort_velocity}/>");
            }
            (_, Some([lo, hi])) => {
                let _ = writeln!(
                    x,
                    "    <limit lower=\"{}\" upper=\"{}\" {effort_velocity}/>",
                    num(lo),
                    num(hi)
                );
            }
            (_, None) => {
                let _ = writeln!(
                    x,
                    "    <!-- warning: no limits in the document; +-{UNLIMITED} m is written -->"
                );
                let _ = writeln!(
                    x,
                    "    <limit lower=\"{}\" upper=\"{}\" {effort_velocity}/>",
                    num(-UNLIMITED),
                    num(UNLIMITED)
                );
            }
        }
    }
    let _ = writeln!(x, "  </joint>");
}

/// `xyz="…" rpy="…"`: the point and the axes of `frame`.
fn origin_at(p: [f64; 3], frame: &Rigid) -> String {
    let rpy = frame.rpy();
    format!(
        "xyz=\"{} {} {}\" rpy=\"{} {} {}\"",
        num(p[0]),
        num(p[1]),
        num(p[2]),
        num(rpy[0]),
        num(rpy[1]),
        num(rpy[2])
    )
}

/// A geometry number: twelve places, trailing zeros and noise dropped.
fn num(x: f64) -> String {
    if !x.is_finite() {
        return "0".into();
    }
    let s = format!("{x:.12}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() {
        "0".into()
    } else {
        s.into()
    }
}

/// A mass or inertia: the shortest text that reads back to the same bits.
fn sci(x: f64) -> String {
    if !x.is_finite() || x == 0.0 {
        "0".into()
    } else if (1e-4..1e9).contains(&x.abs()) {
        format!("{x}")
    } else {
        format!("{x:e}")
    }
}

/// Text safe inside an XML comment.
fn comment(s: &str) -> String {
    sanitize_text(s).replace("--", "- -")
}

fn sanitize_text(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | '&') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::names::sanitize;

    #[test]
    fn numbers_drop_noise_and_negative_zero() {
        assert_eq!(num(0.25), "0.25");
        assert_eq!(num(-1.0e-15), "0");
        assert_eq!(num(3.0), "3");
        assert_eq!(sci(0.0), "0");
        assert_eq!(sci(1.5e-5), "1.5e-5");
        assert_eq!(sanitize("a b"), "a_b");
    }
}
