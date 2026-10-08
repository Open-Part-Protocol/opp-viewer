// SPDX-License-Identifier: Apache-2.0
use crate::package::{Matrix, Package, array, find, multiply, point, text, transform, unit_scale};
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::collections::BTreeMap;
use truck_meshalgo::prelude::*;
use truck_stepio::r#in::{Table, ruststep};

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub triangles: Vec<[[f64; 3]; 3]>,
    pub edges: Vec<([f64; 3], [f64; 3])>,
}

#[derive(Clone, Debug)]
pub struct PartMesh {
    pub node_key: String,
    pub mesh: Mesh,
}

#[derive(Clone, Debug)]
pub struct Scan {
    pub subject_id: String,
    pub name: String,
    pub description: String,
    pub points: Vec<[f64; 3]>,
}

#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub parts: Vec<PartMesh>,
    pub scans: Vec<Scan>,
    pub warnings: Vec<String>,
}

// Read syntax tokens rather than text in STEP comments/string values.
fn step_tokens(input: &str) -> String {
    let data = input.as_bytes();
    let mut output = vec![b' '; data.len()];
    let mut i = 0;
    while i < data.len() {
        if data[i..].starts_with(b"/*") {
            i += 2;
            while i < data.len() && !data[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(data.len());
        } else if data[i] == b'\'' {
            i += 1;
            while i < data.len() {
                if data[i] == b'\'' {
                    if i + 1 < data.len() && data[i + 1] == b'\'' {
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
        } else {
            output[i] = data[i];
            i += 1;
        }
    }
    String::from_utf8_lossy(&output).to_uppercase()
}

pub fn step_mesh(data: &[u8]) -> Result<Mesh> {
    ensure!(
        data.len() <= 16 * 1024 * 1024,
        "STEP preview limit is 16 MiB"
    );
    let input = std::str::from_utf8(data).context("STEP preview requires UTF-8/ASCII Part 21")?;
    let tokens = step_tokens(input);
    ensure!(
        ![
            "NEXT_ASSEMBLY_USAGE_OCCURRENCE",
            "MAPPED_ITEM",
            "REPRESENTATION_RELATIONSHIP_WITH_TRANSFORMATION"
        ]
        .iter()
        .any(|name| tokens.contains(name)),
        "Embedded STEP assembly transforms are not supported by the initial kernel adapter; supply OPP per-component geometry"
    );
    let unit_pattern = regex::Regex::new(
        r"(?s)LENGTH_UNIT\s*\(\s*\)\s+NAMED_UNIT\s*\(\s*\*\s*\)\s+SI_UNIT\s*\(\s*([^,]*)\s*,\s*\.METRE\.\s*\)",
    )?;
    let scales: Vec<_> = unit_pattern
        .captures_iter(&tokens)
        .map(|c| match c[1].trim() {
            ".MILLI." => Ok(1.),
            ".CENTI." => Ok(10.),
            ".MICRO." => Ok(0.001),
            "$" => Ok(1000.),
            prefix => Err(anyhow::anyhow!(
                "Unsupported STEP SI length prefix: {prefix}"
            )),
        })
        .collect::<Result<_>>()?;
    let scale = *scales
        .first()
        .context("Cannot resolve an explicit STEP SI length unit; preview omitted")?;
    ensure!(
        scales.iter().all(|v| *v == scale),
        "Multiple STEP length unit contexts are not supported by this preview"
    );
    let exchange =
        ruststep::parser::parse(input).map_err(|e| anyhow::anyhow!("STEP parse failed: {e}"))?;
    ensure!(
        exchange.data.len() == 1,
        "Multiple STEP data sections are not supported by this preview"
    );
    let table = Table::from_data_section(&exchange.data[0]);
    ensure!(
        !table.shell.is_empty(),
        "No supported STEP B-rep shells found"
    );
    let mut shells: Vec<_> = table.shell.iter().collect();
    shells.sort_by_key(|(id, _)| **id);
    let mut result = Mesh::default();
    for (_, holder) in shells {
        let shell = table
            .to_compressed_shell(holder)
            .map_err(|e| anyhow::anyhow!("STEP shell conversion failed: {e}"))?;
        ensure!(
            shell.faces.len() == holder.cfs_faces.len(),
            "Some STEP faces could not be converted; incomplete preview omitted"
        );
        let meshed = shell.robust_triangulation(0.01 / scale);
        ensure!(
            meshed.faces.iter().all(|face| face.surface.is_some()),
            "A STEP face could not be triangulated; incomplete preview omitted"
        );
        let mut polygon = meshed.to_polygon();
        polygon.triangulate();
        for face in polygon.tri_faces() {
            let points = face.map(|vertex| {
                let p = polygon.positions()[vertex.pos];
                [p.x * scale, p.y * scale, p.z * scale]
            });
            ensure!(
                points.iter().flatten().all(|v| v.is_finite()),
                "Nonfinite mesh coordinates"
            );
            result.triangles.push(points);
            ensure!(
                result.triangles.len() <= 50_000,
                "Preview exceeds 50,000 triangles; use a lower-detail representation"
            );
        }
        for edge in meshed.edges {
            for pair in edge.curve.0.windows(2) {
                result.edges.push((
                    [pair[0].x * scale, pair[0].y * scale, pair[0].z * scale],
                    [pair[1].x * scale, pair[1].y * scale, pair[1].z * scale],
                ));
            }
        }
    }
    ensure!(
        !result.triangles.is_empty(),
        "STEP tessellation produced no faces"
    );
    Ok(result)
}

fn placed(mesh: &Mesh, matrix: Matrix) -> Mesh {
    Mesh {
        triangles: mesh
            .triangles
            .iter()
            .map(|t| t.map(|p| point(matrix, p)))
            .collect(),
        edges: mesh
            .edges
            .iter()
            .map(|(a, b)| (point(matrix, *a), point(matrix, *b)))
            .collect(),
    }
}

pub fn ply_points(data: &[u8], unit: &str) -> Result<Vec<[f64; 3]>> {
    ensure!(
        data.len() <= 16 * 1024 * 1024,
        "PLY preview limit is 16 MiB"
    );
    let input = std::str::from_utf8(data)?;
    let (header, body) = input
        .split_once("end_header")
        .context("Missing PLY header")?;
    ensure!(
        header.len() <= 65536 && header.starts_with("ply") && header.contains("format ascii 1.0"),
        "Only ASCII PLY previews are supported"
    );
    let mut properties = Vec::new();
    let mut count = 0usize;
    let mut vertices = false;
    for line in header.lines() {
        let tokens: Vec<_> = line.split_whitespace().collect();
        if tokens.starts_with(&["element", "vertex"]) {
            count = tokens.get(2).context("Missing PLY vertex count")?.parse()?;
            vertices = true;
        } else if tokens.starts_with(&["element"]) {
            vertices = false;
        } else if vertices && tokens.starts_with(&["property"]) {
            ensure!(tokens.len() == 3, "PLY list properties are unsupported");
            properties.push(tokens[2]);
        }
    }
    ensure!(
        (1..=100_000).contains(&count),
        "PLY previews require 1–100,000 vertices"
    );
    let indices = ["x", "y", "z"]
        .map(|axis| {
            properties
                .iter()
                .position(|name| *name == axis)
                .context("PLY is missing x/y/z properties")
        })
        .into_iter()
        .collect::<Result<Vec<_>>>()?;
    let scale = unit_scale(unit)?;
    let points = body
        .lines()
        .filter(|line| !line.trim().is_empty())
        .take(count)
        .map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            let coords = indices
                .iter()
                .map(|i| {
                    Ok(fields
                        .get(*i)
                        .context("Incomplete PLY vertex")?
                        .parse::<f64>()?
                        * scale)
                })
                .collect::<Result<Vec<_>>>()?;
            ensure!(
                coords.iter().all(|v| v.is_finite()),
                "Nonfinite scan coordinates"
            );
            Ok([coords[0], coords[1], coords[2]])
        })
        .collect::<Result<Vec<_>>>()?;
    ensure!(
        points.len() == count,
        "PLY has fewer vertices than declared"
    );
    Ok(points)
}

impl Scene {
    pub fn from_package(package: &Package) -> Self {
        let mut scene = Self::default();
        let mut cache: BTreeMap<String, std::result::Result<Mesh, String>> = BTreeMap::new();
        for node in &package.nodes {
            let reps: Vec<_> = array(&package.design, "representations")
                .iter()
                .filter(|r| {
                    text(r, "productDefinitionId") == node.definition_id
                        && text(r, "role") == "nominal-exact"
                })
                .collect();
            if reps.len() > 1 {
                scene.warnings.push(format!("{}: first nominal state shown; other representations remain available in Package",node.name));
            }
            let Some(rep) = reps.first() else {
                continue;
            };
            if text(rep, "encoding") != "step-part21" {
                scene.warnings.push(format!(
                    "{}: {} preview unsupported",
                    node.name,
                    text(rep, "encoding")
                ));
                continue;
            }
            let resource_id = text(rep, "resourceId").to_owned();
            let mesh = cache.entry(resource_id.clone()).or_insert_with(|| {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let frame = find(&package.design, "coordinateFrames", text(rep, "frameId"))?;
                    ensure!(
                        frame["parentFrameId"].is_null() && frame["transform"].is_null(),
                        "Nested geometry coordinate frames are not supported by the preview adapter"
                    );
                    step_mesh(package.resource(&resource_id)?.1)
                }));
                match result {
                    Ok(result) => result.map_err(|e| e.to_string()),
                    Err(_) => Err(
                        "Geometry kernel rejected this resource; metadata remains reviewable"
                            .into(),
                    ),
                }
            });
            match mesh {
                Ok(mesh) => scene.parts.push(PartMesh {
                    node_key: node.key.clone(),
                    mesh: placed(mesh, node.world),
                }),
                Err(message) => scene.warnings.push(format!("{}: {message}", node.name)),
            }
        }
        if let Some(actual) = &package.actual {
            for scan in array(actual, "scans") {
                let result = (|| -> Result<Vec<Scan>> {
                    ensure!(
                        text(scan, "dataClass") == "point-cloud",
                        "Only point-cloud scans have a preview"
                    );
                    ensure!(
                        text(&scan["registration"], "method") != "unregistered",
                        "Scan has no usable registration"
                    );
                    let frame = find(
                        &package.design,
                        "coordinateFrames",
                        text(&scan["registration"], "designFrameId"),
                    )?;
                    ensure!(
                        frame["parentFrameId"].is_null() && frame["transform"].is_null(),
                        "Nested scan registration frames are not supported"
                    );
                    let points = ply_points(
                        package.resource(text(scan, "resourceId"))?.1,
                        text(scan, "unit"),
                    )?;
                    let mut previews = Vec::new();
                    for id in array(scan, "subjectIds") {
                        let subject = find(actual, "subjects", id.as_str().unwrap_or(""))?;
                        let path: Vec<_> = array(subject, "occurrencePath")
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect();
                        let node = package
                            .nodes
                            .iter()
                            .find(|n| n.path == path)
                            .context("Scan occurrence path unavailable")?;
                        ensure!(
                            node.definition_id == text(frame, "productDefinitionId"),
                            "Scan frame belongs to another product definition"
                        );
                        let matrix =
                            multiply(node.world, transform(&scan["registration"]["transform"])?);
                        previews.push(Scan {
                            subject_id: id.as_str().unwrap_or("").into(),
                            name: text(scan, "id").into(),
                            description: text(scan, "description").into(),
                            points: points.iter().map(|p| point(matrix, *p)).collect(),
                        });
                    }
                    Ok(previews)
                })();
                match result {
                    Ok(previews) => scene.scans.extend(previews),
                    Err(e) => scene.warnings.push(format!("{}: {e}", text(scan, "id"))),
                }
            }
        }
        scene
    }

    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in self
            .parts
            .iter()
            .flat_map(|part| part.mesh.triangles.iter().flatten())
        {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        if min[0].is_finite() {
            (min, max)
        } else {
            ([0.; 3], [40., 20., 10.])
        }
    }
}
