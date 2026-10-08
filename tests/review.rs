// SPDX-License-Identifier: Apache-2.0
use opp_viewer::{
    geometry::{Scene, ply_points, step_mesh},
    package::{Package, hash, parse_json, read_archive, text, transform},
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
};
use zip::{ZipWriter, write::SimpleFileOptions};

const BLOCK: &[u8] = include_bytes!("../fixtures/block-as-built.opp");
const ASSEMBLY: &[u8] = include_bytes!("../fixtures/assembly-as-built.opp");

fn archive(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, data) in files {
        writer
            .start_file(
                path,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

// Alter a document and update only its transport hash. Immutable design pins remain independent.
fn mutate(bytes: &[u8], role: &str, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut files = read_archive(bytes).unwrap();
    let mut manifest = parse_json(&files["manifest.json"]).unwrap();
    let resource = manifest["resources"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| text(r, "role") == role)
        .unwrap();
    let path = text(resource, "path").to_owned();
    let mut document = parse_json(&files[&path]).unwrap();
    edit(&mut document);
    let data = serde_json::to_vec(&document).unwrap();
    resource["sha256"] = hash(&data).into();
    resource["size"] = data.len().into();
    files.insert(path, data);
    files.insert(
        "manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    archive(&files)
}

fn rejects(bytes: &[u8], expected: &str) {
    let error = Package::from_bytes(bytes, "invalid.opp")
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(expected),
        "expected {expected:?}, got {error:?}"
    );
}

#[test]
fn all_protocol_examples_open_without_network() {
    for (name, bytes) in [
        (
            "block-design",
            include_bytes!("../fixtures/block-design.opp").as_slice(),
        ),
        ("block-as-built", BLOCK),
        (
            "assembly-design",
            include_bytes!("../fixtures/assembly-design.opp").as_slice(),
        ),
        ("assembly-as-built", ASSEMBLY),
        (
            "lot-as-built",
            include_bytes!("../fixtures/lot-as-built.opp").as_slice(),
        ),
    ] {
        let package = Package::from_bytes(bytes, name).unwrap_or_else(|e| panic!("{name}: {e:#}"));
        assert_eq!(package.report()["engineeringCertification"], "not-assessed");
        assert!(!package.warnings.is_empty());
    }
}

#[test]
fn actual_exceptions_preserve_failed_measurement() {
    let package = Package::from_bytes(BLOCK, "block.opp").unwrap();
    let actual = package.actual.unwrap();
    let e = actual["evaluations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| text(r, "requirementId") == "req-width")
        .unwrap();
    assert_eq!(e["conformance"], "fail");
    assert_eq!(e["disposition"], "accepted-under-deviation");
    let o = actual["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| text(r, "requirementId") == "req-width")
        .unwrap();
    assert_eq!(o["value"]["quantity"]["value"], "20.060");
}

#[test]
fn displays_latest_evaluation_with_its_own_retest_measurement() {
    let mut package = Package::from_bytes(BLOCK, "block.opp").unwrap();
    let actual = package.actual.as_mut().unwrap();
    let mut observation = actual["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| text(o, "requirementId") == "req-width")
        .unwrap()
        .clone();
    observation["id"] = "retest-width".into();
    observation["observedAt"] = "2026-10-03T10:00:00-04:00".into();
    observation["value"]["quantity"]["value"] = "20.010".into();
    actual["observations"]
        .as_array_mut()
        .unwrap()
        .push(observation);
    let mut evaluation = actual["evaluations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| text(e, "requirementId") == "req-width")
        .unwrap()
        .clone();
    evaluation["id"] = "evaluation-retest-width".into();
    evaluation["evaluatedAt"] = "2026-10-03T16:00:00-04:00".into();
    evaluation["observationIds"] = serde_json::json!(["retest-width"]);
    evaluation["conformance"] = "pass".into();
    actual["evaluations"]
        .as_array_mut()
        .unwrap()
        .push(evaluation);
    assert_eq!(
        package
            .latest_evaluation("req-width", "physical-block")
            .unwrap()["id"],
        "evaluation-retest-width"
    );
    assert_eq!(
        package
            .review_observation("req-width", "physical-block")
            .unwrap()["value"]["quantity"]["value"],
        "20.010"
    );
    assert!(
        package
            .latest_evaluation("req-width", "another-subject")
            .is_none()
    );
}

#[test]
fn rejects_corrupt_transport_hash() {
    let mut files = read_archive(BLOCK).unwrap();
    files.get_mut("evidence/test.csv").unwrap().push(b'!');
    rejects(&archive(&files), "SHA-256 mismatch");
}

#[test]
fn rejects_design_geometry_change_with_rehashed_inventory() {
    let mut files = read_archive(BLOCK).unwrap();
    let mut manifest = parse_json(&files["manifest.json"]).unwrap();
    let r = manifest["resources"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|r| text(r, "role") == "nominal-geometry")
        .unwrap();
    let data = files.get_mut(text(r, "path")).unwrap();
    data.extend_from_slice(b"\n/* changed */");
    r["size"] = data.len().into();
    r["sha256"] = hash(data).into();
    files.insert(
        "manifest.json".into(),
        serde_json::to_vec(&manifest).unwrap(),
    );
    rejects(&archive(&files), "Design snapshot pin mismatch");
}

#[test]
fn rejects_unpinned_design_resource() {
    let original = include_bytes!("../fixtures/block-design.opp");
    let modified = mutate(original, "design", |d| {
        d["resourcePins"]
            .as_array_mut()
            .unwrap()
            .retain(|p| text(p, "resourceId") != "res-block-step");
    });
    rejects(&modified, "lacks a hash pin");
}

#[test]
fn rejects_false_numeric_pass() {
    let bytes = mutate(BLOCK, "as-built", |a| {
        let e = a["evaluations"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| text(e, "requirementId") == "req-width")
            .unwrap();
        e["conformance"] = "pass".into();
    });
    rejects(&bytes, "contradicts");
}

#[test]
fn rejects_expired_calibration() {
    let bytes = mutate(BLOCK, "as-built", |a| {
        a["calibrations"][0]["validUntil"] = "2026-06-01T00:00:00Z".into();
    });
    rejects(&bytes, "Calibration is not valid");
}

#[test]
fn rejects_expired_deviation() {
    let bytes = mutate(BLOCK, "as-built", |a| {
        a["deviations"][0]["expiresAt"] = "2026-10-02T15:30:00-04:00".into();
    });
    rejects(&bytes, "Deviation has expired");
}

#[test]
fn rejects_cross_subject_evaluation() {
    let bytes = mutate(ASSEMBLY, "as-built", |a| {
        let subjects = a["subjects"].as_array().unwrap();
        let first = text(&a["evaluations"][0], "subjectId");
        let other = subjects
            .iter()
            .map(|s| text(s, "id"))
            .find(|id| *id != first)
            .unwrap()
            .to_owned();
        a["evaluations"][0]["subjectId"] = other.into();
    });
    rejects(&bytes, "another subject");
}

#[test]
fn distinguishes_reused_assembly_occurrence_paths() {
    let package = Package::from_bytes(ASSEMBLY, "assembly.opp").unwrap();
    assert_eq!(package.nodes.len(), 5);
    let repeated: Vec<_> = package.nodes.iter().filter(|n| n.path.len() == 2).collect();
    assert_eq!(repeated.len(), 2);
    assert_eq!(repeated[0].definition_id, repeated[1].definition_id);
    assert_ne!(repeated[0].key, repeated[1].key);
    assert_ne!(repeated[0].world, repeated[1].world);
}

#[test]
fn rejects_incomplete_occurrence_path() {
    let bytes = mutate(ASSEMBLY, "as-built", |a| {
        let subject = a["subjects"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|s| s["occurrencePath"].as_array().unwrap().len() == 2)
            .unwrap();
        subject["occurrencePath"].as_array_mut().unwrap().remove(0);
    });
    rejects(&bytes, "invalid occurrence path");
}

#[test]
fn rejects_unsafe_and_colliding_zip_paths() {
    for paths in [
        vec!["../escape.json"],
        vec!["a\\b.json"],
        vec!["/absolute.json"],
        vec!["A.json", "a.json"],
        vec!["dir", "dir/file.json"],
    ] {
        let files = paths
            .into_iter()
            .map(|p| (p.into(), b"{}".to_vec()))
            .collect();
        assert!(read_archive(&archive(&files)).is_err());
    }
}

#[test]
fn rejects_archive_symlinks() {
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    zip.add_symlink("link", "outside", SimpleFileOptions::default())
        .unwrap();
    assert!(
        read_archive(&zip.finish().unwrap().into_inner())
            .unwrap_err()
            .to_string()
            .contains("symlinks")
    );
}

#[test]
fn rejects_duplicate_json_keys() {
    assert!(parse_json(br#"{"measurement":{"value":"20","value":"21"}}"#).is_err());
}

#[test]
fn rejects_unknown_schema_properties() {
    let bytes = mutate(BLOCK, "as-built", |a| {
        a["inventedField"] = true.into();
    });
    rejects(&bytes, "schema check");
}

#[test]
fn converts_actual_step_geometry_and_registered_scan() {
    let package = Package::from_bytes(BLOCK, "block.opp").unwrap();
    let scene = Scene::from_package(&package);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    assert_eq!(scene.parts.len(), 1);
    assert_eq!(scene.scans.len(), 1);
    assert_eq!(scene.scans[0].points.len(), 8);
    let (min, max) = scene.bounds();
    for (got, want) in min.into_iter().chain(max).zip([0., 0., 0., 40., 20., 10.]) {
        assert!((got - want).abs() < 1e-6, "{got} != {want}");
    }
}

#[test]
fn applies_opp_assembly_transforms_to_preview() {
    let package = Package::from_bytes(ASSEMBLY, "assembly.opp").unwrap();
    let scene = Scene::from_package(&package);
    assert!(scene.warnings.is_empty(), "{:?}", scene.warnings);
    assert_eq!(scene.parts.len(), 2);
    let (_, max) = scene.bounds();
    assert!((max[0] - 100.).abs() < 1e-6);
    assert_eq!(scene.scans.len(), 2);
    assert_ne!(scene.scans[0].subject_id, scene.scans[1].subject_id);
    assert!((scene.scans[1].points[0][0] - scene.scans[0].points[0][0] - 60.).abs() < 1e-6);
}

#[test]
fn omits_step_preview_when_units_are_ambiguous() {
    let package = Package::from_bytes(BLOCK, "block.opp").unwrap();
    let (_, bytes) = package.resource("res-block-step").unwrap();
    let input = String::from_utf8(bytes.to_vec()).unwrap();
    let unknown = input.replace(".MILLI.,.METRE.", ".KILO.,.METRE.");
    assert!(step_mesh(unknown.as_bytes()).is_err());
    let missing = input.replace("LENGTH_UNIT()", "LENGTH_UNIT_MISSING()");
    assert!(step_mesh(missing.as_bytes()).is_err());
}

#[test]
fn unsupported_step_preview_preserves_metadata() {
    let mut package = Package::from_bytes(BLOCK, "block.opp").unwrap();
    let path = package.resources["res-block-step"].path.clone();
    package.files.insert(path, b"unsupported".to_vec());
    let scene = Scene::from_package(&package);
    assert!(scene.parts.is_empty());
    assert!(!scene.warnings.is_empty());
    assert!(
        !package.design["requirements"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn scan_units_and_finite_coordinates_are_required() {
    let ply=b"ply\nformat ascii 1.0\nelement vertex 1\nproperty float x\nproperty float y\nproperty float z\nend_header\n1 2 3\n";
    assert_eq!(ply_points(ply, "m").unwrap(), vec![[1000., 2000., 3000.]]);
    assert!(ply_points(ply, "invalid-unit").is_err());
    assert!(
        ply_points(
            &String::from_utf8(ply.to_vec())
                .unwrap()
                .replace("1 2 3", "NaN 2 3")
                .into_bytes(),
            "mm"
        )
        .is_err()
    );
}

#[test]
fn rejects_reflected_or_scaled_occurrence_matrices() {
    for rotation in [
        [-1., 0., 0., 0., 1., 0., 0., 0., 1.],
        [2., 0., 0., 0., 1., 0., 0., 0., 1.],
    ] {
        let value = serde_json::json!({"rotation":rotation.map(|v|v.to_string()),"translation":["0","0","0"],"translationUnit":"mm"});
        assert!(transform(&value).is_err());
    }
}
