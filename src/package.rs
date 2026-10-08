// SPDX-License-Identifier: Apache-2.0
use anyhow::{Context, Result, anyhow, bail, ensure};
use chrono::DateTime;
use rust_decimal::Decimal;
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    io::{Cursor, Read},
    path::Path,
    str::FromStr,
};

pub const VERSION: &str = "0.1.0-draft.1";
const MAX_ARCHIVE: usize = 64 * 1024 * 1024;
const MAX_ENTRY: u64 = 64 * 1024 * 1024;
const MAX_TOTAL: u64 = 256 * 1024 * 1024;
pub type Matrix = [[f64; 4]; 4];

pub fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
pub fn array<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}
pub fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub path: String,
    pub role: String,
    pub media_type: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub key: String,
    pub name: String,
    pub definition_id: String,
    pub path: Vec<String>,
    pub world: Matrix,
}

#[derive(Debug, Clone)]
pub struct Package {
    pub filename: String,
    pub manifest: Value,
    pub design: Value,
    pub actual: Option<Value>,
    pub bindings: Value,
    pub documents: BTreeMap<String, Value>,
    pub resources: BTreeMap<String, Resource>,
    pub files: BTreeMap<String, Vec<u8>>,
    pub nodes: Vec<Node>,
    pub warnings: Vec<String>,
}

// serde_json normally keeps the last duplicate object key. Exchange records must reject it.
struct StrictValue(Value);
impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictValue;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Bool(v)))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                Number::from_f64(v)
                    .map(|n| StrictValue(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(StrictValue(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(StrictValue(v)) = seq.next_element()? {
                    values.push(v);
                }
                Ok(StrictValue(Value::Array(values)))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some((k, StrictValue(v))) = map.next_entry::<String, StrictValue>()? {
                    if values.insert(k.clone(), v).is_some() {
                        return Err(serde::de::Error::custom(format!("duplicate JSON key: {k}")));
                    }
                }
                Ok(StrictValue(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}

pub fn parse_json(bytes: &[u8]) -> Result<Value> {
    ensure!(
        bytes.len() <= 8 * 1024 * 1024,
        "JSON resource exceeds this viewer's 8 MiB limit"
    );
    Ok(serde_json::from_slice::<StrictValue>(bytes)?.0)
}

pub fn read_archive(bytes: &[u8]) -> Result<BTreeMap<String, Vec<u8>>> {
    ensure!(
        bytes.len() <= MAX_ARCHIVE,
        "Package exceeds this viewer's 64 MiB compressed limit"
    );
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))?;
    ensure!(
        zip.len() <= 2048,
        "Package has more than 2,048 archive entries"
    );
    let mut files = BTreeMap::new();
    let mut paths = BTreeSet::new();
    let mut total = 0u64;
    let safe = regex::Regex::new(r"^[A-Za-z0-9_-]+(?:/[A-Za-z0-9_-]+)*(?:\.[A-Za-z0-9_-]+)?$")?;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        ensure!(
            !entry.encrypted(),
            "Encrypted archive entries are forbidden"
        );
        let name = entry.name().trim_end_matches('/').to_owned();
        ensure!(safe.is_match(&name), "Unsafe archive path: {name}");
        ensure!(
            paths.insert(name.to_lowercase()),
            "Duplicate or case-colliding archive path: {name}"
        );
        ensure!(
            entry
                .unix_mode()
                .map(|m| m & 0o170000 != 0o120000)
                .unwrap_or(true),
            "Archive symlinks are forbidden"
        );
        ensure!(
            matches!(
                entry.compression(),
                zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
            ),
            "Unsupported ZIP compression"
        );
        total = total
            .checked_add(entry.size())
            .context("Expanded archive size overflow")?;
        ensure!(
            entry.size() <= MAX_ENTRY && total <= MAX_TOTAL,
            "Expanded package exceeds viewer limits"
        );
        ensure!(
            entry.size() <= entry.compressed_size().max(1).saturating_mul(200),
            "Archive exceeds compression-ratio limit"
        );
        if entry.is_dir() {
            continue;
        }
        let expected = entry.size();
        let mut data = Vec::new();
        entry.by_ref().take(MAX_ENTRY + 1).read_to_end(&mut data)?;
        ensure!(
            data.len() as u64 == expected && data.len() as u64 <= MAX_ENTRY,
            "Archive member size mismatch"
        );
        files.insert(name, data);
    }
    for path in &paths {
        let parts: Vec<_> = path.split('/').collect();
        for i in 1..parts.len() {
            ensure!(
                !files
                    .keys()
                    .any(|k| k.to_lowercase() == parts[..i].join("/")),
                "File/directory prefix collision"
            );
        }
    }
    Ok(files)
}

#[derive(Clone)]
struct Schemas(BTreeMap<String, Value>);
impl jsonschema::Retrieve for Schemas {
    fn retrieve(
        &self,
        uri: &jsonschema::Uri<String>,
    ) -> std::result::Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        self.0
            .get(uri.as_str())
            .cloned()
            .ok_or_else(|| format!("Unbundled schema: {uri}").into())
    }
}

fn schemas() -> Result<Schemas> {
    let mut values = BTreeMap::new();
    for data in [
        include_str!("../protocol/schemas/v0/common.schema.json"),
        include_str!("../protocol/schemas/v0/manifest.schema.json"),
        include_str!("../protocol/schemas/v0/design.schema.json"),
        include_str!("../protocol/schemas/v0/as-built.schema.json"),
        include_str!("../protocol/schemas/v0/bindings.schema.json"),
        include_str!("../protocol/schemas/v0/presentation.schema.json"),
        include_str!("../protocol/schemas/v0/conversion-report.schema.json"),
    ] {
        let schema = parse_json(data.as_bytes())?;
        values.insert(text(&schema, "$id").to_owned(), schema);
    }
    Ok(Schemas(values))
}

fn validate(value: &Value, kind: &str, schemas: &Schemas) -> Result<()> {
    let id = format!("urn:opp:schema:{VERSION}:{kind}");
    let schema = schemas.0.get(&id).context("Missing embedded schema")?;
    let validator = jsonschema::draft202012::options()
        .with_retriever(schemas.clone())
        .should_validate_formats(true)
        .build(schema)?;
    let errors: Vec<_> = validator
        .iter_errors(value)
        .take(6)
        .map(|e| format!("{}: {}", e.instance_path(), e))
        .collect();
    ensure!(
        errors.is_empty(),
        "{kind} schema check: {}",
        errors.join("; ")
    );
    Ok(())
}

impl Package {
    pub fn open(path: &Path) -> Result<Self> {
        ensure!(
            path.extension()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.eq_ignore_ascii_case("opp")),
            "Choose an .opp package"
        );
        ensure!(
            path.metadata()?.len() <= MAX_ARCHIVE as u64,
            "Package exceeds the compressed size limit"
        );
        Self::from_bytes(
            &std::fs::read(path)?,
            &path.file_name().unwrap_or_default().to_string_lossy(),
        )
    }

    pub fn from_bytes(bytes: &[u8], filename: &str) -> Result<Self> {
        let files = read_archive(bytes)?;
        let bundled = schemas()?;
        let manifest = parse_json(
            files
                .get("manifest.json")
                .context("Missing root manifest.json")?,
        )?;
        validate(&manifest, "manifest", &bundled)?;
        ensure!(
            text(&manifest, "formatVersion") == VERSION,
            "Unsupported OPP format version"
        );
        let mut resources = BTreeMap::new();
        let mut inventoried = BTreeSet::from(["manifest.json".to_owned()]);
        for resource in array(&manifest, "resources") {
            let resource: Resource = serde_json::from_value(resource.clone())?;
            ensure!(
                inventoried.insert(resource.path.clone()),
                "Duplicate inventoried path"
            );
            let data = files
                .get(&resource.path)
                .with_context(|| format!("Missing resource: {}", resource.path))?;
            ensure!(
                data.len() as u64 == resource.size && hash(data) == resource.sha256,
                "Size/SHA-256 mismatch: {}",
                resource.path
            );
            ensure!(
                resources.insert(resource.id.clone(), resource).is_none(),
                "Duplicate resource ID"
            );
        }
        ensure!(
            inventoried == files.keys().cloned().collect(),
            "Package contains uninventoried files"
        );
        let mut documents = BTreeMap::new();
        for key in [
            "designResourceId",
            "bindingsResourceId",
            "asBuiltResourceId",
            "presentationResourceId",
        ] {
            if manifest[key].is_null() {
                continue;
            }
            let id = text(&manifest, key);
            let resource = resources
                .get(id)
                .context("Missing root document resource")?;
            let role = match key {
                "designResourceId" => "design",
                "bindingsResourceId" => "bindings",
                "asBuiltResourceId" => "as-built",
                _ => "presentation",
            };
            ensure!(resource.role == role, "Wrong root document role for {key}");
            let doc = parse_json(&files[&resource.path])?;
            validate(&doc, role, &bundled)?;
            documents.insert(id.to_owned(), doc);
        }
        for resource in resources.values().filter(|r| r.role == "conversion-report") {
            let doc = parse_json(&files[&resource.path])?;
            validate(&doc, "conversion-report", &bundled)?;
            documents.insert(resource.id.clone(), doc);
        }
        let design = documents
            .get(text(&manifest, "designResourceId"))
            .context("Missing design")?
            .clone();
        let bindings = documents
            .get(text(&manifest, "bindingsResourceId"))
            .context("Missing bindings")?
            .clone();
        let actual = documents.get(text(&manifest, "asBuiltResourceId")).cloned();
        ensure!(
            (text(&manifest, "packageType") == "as-built") == actual.is_some(),
            "Package type and as-built document disagree"
        );
        let mut package = Self {
            filename: filename.to_owned(),
            manifest,
            design,
            actual,
            bindings,
            documents,
            resources,
            files,
            nodes: Vec::new(),
            warnings: Vec::new(),
        };
        package.check_links()?;
        package.check_pins()?;
        package.expand_tree()?;
        package.check_actual()?;
        for unresolved in array(&package.design["release"], "unresolvedItems") {
            if let Some(s) = unresolved.as_str() {
                package.warnings.push(s.to_owned());
            }
        }
        let mut profile_ids = BTreeSet::new();
        for profile in array(&package.manifest, "profiles") {
            ensure!(
                profile_ids.insert(text(profile, "id")),
                "Duplicate profile ID"
            );
            if profile["required"] == true
                && (![
                    "opp.core",
                    "opp.geometry.step-part21",
                    "opp.dimensions",
                    "opp.gdt.asme",
                    "opp.gdt.iso",
                    "opp.requirements",
                    "opp.as-built",
                    "opp.scan",
                    "opp.fai",
                ]
                .contains(&text(profile, "id"))
                    || text(profile, "version") != VERSION)
            {
                package.warnings.push(format!(
                    "Unknown required profile/version: {} {}",
                    text(profile, "id"),
                    text(profile, "version")
                ));
            }
        }
        fn extensions(v: &Value, warnings: &mut Vec<String>) {
            match v {
                Value::Object(map) => {
                    if map.contains_key("namespace")
                        && map.contains_key("data")
                        && map.get("required") == Some(&Value::Bool(true))
                    {
                        warnings.push(format!(
                            "Required extension semantics not interpreted: {}",
                            text(v, "namespace")
                        ));
                    }
                    for value in map.values() {
                        extensions(value, warnings);
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        extensions(value, warnings);
                    }
                }
                _ => (),
            }
        }
        extensions(&package.design, &mut package.warnings);
        if let Some(actual) = &package.actual {
            extensions(actual, &mut package.warnings);
        }
        package.warnings.push("Engineering interpretation, authenticity, full GD&T and AS9102 compliance are not certified by this viewer.".into());
        Ok(package)
    }

    fn check_pins(&self) -> Result<()> {
        let mut pins = BTreeSet::new();
        for pin in array(&self.design, "resourcePins") {
            ensure!(
                pins.insert(text(pin, "resourceId").to_owned()),
                "Duplicate design resource pin"
            );
            ensure!(
                text(pin, "resourceId") != text(&self.manifest, "designResourceId"),
                "Design cannot pin itself"
            );
            let resource = self
                .resources
                .get(text(pin, "resourceId"))
                .context("Missing design resource pin")?;
            ensure!(
                text(pin, "sha256") == resource.sha256,
                "Design snapshot pin mismatch: {}",
                resource.path
            );
        }
        fn closure(v: &Value, resources: &mut BTreeSet<String>) {
            match v {
                Value::Object(map) => {
                    for (key, value) in map {
                        if matches!(key.as_str(), "extensions" | "resourcePins") {
                            continue;
                        }
                        if matches!(
                            key.as_str(),
                            "resourceId"
                                | "sourceResourceId"
                                | "boundaryResourceId"
                                | "previewResourceId"
                                | "certificateResourceId"
                                | "evidenceResourceIds"
                                | "budgetResourceId"
                                | "settingsResourceId"
                        ) {
                            if let Some(s) = value.as_str() {
                                resources.insert(s.into());
                            }
                            if let Some(values) = value.as_array() {
                                resources.extend(
                                    values.iter().filter_map(Value::as_str).map(str::to_owned),
                                );
                            }
                        }
                        closure(value, resources);
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        closure(value, resources);
                    }
                }
                _ => (),
            }
        }
        let mut required = BTreeSet::from([text(&self.manifest, "bindingsResourceId").to_owned()]);
        closure(&self.design, &mut required);
        closure(&self.bindings, &mut required);
        ensure!(
            required.is_subset(&pins),
            "Design closure resource lacks a hash pin"
        );
        ensure!(
            text(&self.bindings, "designId") == text(&self.design, "id"),
            "Binding design ID mismatch"
        );
        for binding in array(&self.bindings, "bindings") {
            let resource = self
                .resources
                .get(text(binding, "resourceId"))
                .context("Binding resource missing")?;
            ensure!(
                text(binding, "resourceSha256") == resource.sha256,
                "Binding resource hash mismatch"
            );
        }
        if let Some(actual) = &self.actual {
            let snapshot = &actual["designSnapshot"];
            let design_id = text(&self.manifest, "designResourceId");
            ensure!(
                text(snapshot, "designId") == text(&self.design, "id")
                    && text(snapshot, "designResourceId") == design_id
                    && text(snapshot, "sha256") == self.resources[design_id].sha256,
                "As-built design snapshot mismatch"
            );
        }
        Ok(())
    }

    fn check_links(&self) -> Result<()> {
        fn index(v: &Value) -> Result<BTreeMap<String, BTreeSet<String>>> {
            let mut result = BTreeMap::new();
            let mut all = BTreeSet::new();
            for (key, values) in v.as_object().context("Document must be an object")? {
                if matches!(key.as_str(), "extensions" | "resourcePins") {
                    continue;
                }
                if let Some(values) = values.as_array() {
                    let mut ids = BTreeSet::new();
                    for value in values {
                        if let Some(id) = value["id"].as_str() {
                            ensure!(all.insert(id.to_owned()), "Duplicate record ID: {id}");
                            ids.insert(id.to_owned());
                        }
                    }
                    result.insert(key.clone(), ids);
                }
            }
            Ok(result)
        }
        let design = index(&self.design)?;
        let actual = self
            .actual
            .as_ref()
            .map(index)
            .transpose()?
            .unwrap_or_default();
        let resource_ids: BTreeSet<_> = self.resources.keys().cloned().collect();
        let binding_ids = array(&self.bindings, "bindings")
            .iter()
            .map(|b| text(b, "id").to_owned())
            .collect();
        let mappings = [
            ("rootDefinitionId", "productDefinitions"),
            ("productDefinitionId", "productDefinitions"),
            ("parentDefinitionId", "productDefinitions"),
            ("childDefinitionId", "productDefinitions"),
            ("stateId", "states"),
            ("frameId", "coordinateFrames"),
            ("designFrameId", "coordinateFrames"),
            ("representationId", "representations"),
            ("regionId", "regions"),
            ("regionIds", "regions"),
            ("featureId", "features"),
            ("materialDefinitionId", "materials"),
            ("datumSystemId", "datumSystems"),
            ("datumId", "datums"),
            ("requirementId", "requirements"),
            ("requirementIds", "requirements"),
            ("overridesRequirementIds", "requirements"),
            ("verificationId", "verifications"),
            ("interpretationId", "interpretations"),
            ("decisionRuleId", "decisionRules"),
        ];
        let actual_mappings = [
            ("subjectId", "subjects"),
            ("subjectIds", "subjects"),
            ("parentSubjectId", "subjects"),
            ("runId", "runs"),
            ("equipmentId", "equipment"),
            ("equipmentIds", "equipment"),
            ("calibrationIds", "calibrations"),
            ("observationIds", "observations"),
            ("evaluationIds", "evaluations"),
            ("deviationId", "deviations"),
            ("deviationIds", "deviations"),
            ("materialLotIds", "materialLots"),
            ("productionEventIds", "productionEvents"),
            ("cellId", "productionCells"),
            ("approvalIds", "approvals"),
        ];
        fn visit(v: &Value, refs: &BTreeMap<&str, &BTreeSet<String>>) -> Result<()> {
            match v {
                Value::Object(values) => {
                    for (key, value) in values {
                        if key == "extensions" {
                            continue;
                        }
                        if let Some(target) = refs.get(key.as_str()) {
                            let items: Vec<_> = if let Some(values) = value.as_array() {
                                values.iter().filter_map(Value::as_str).collect()
                            } else {
                                value.as_str().into_iter().collect()
                            };
                            for id in items {
                                ensure!(target.contains(id), "Unresolved {key}: {id}");
                            }
                        }
                        visit(value, refs)?;
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        visit(value, refs)?;
                    }
                }
                _ => (),
            }
            Ok(())
        }
        let empty = BTreeSet::new();
        let mut refs: BTreeMap<_, _> = mappings
            .iter()
            .map(|(key, collection)| (*key, design.get(*collection).unwrap_or(&empty)))
            .collect();
        for key in [
            "resourceId",
            "sourceResourceId",
            "boundaryResourceId",
            "previewResourceId",
            "budgetResourceId",
            "settingsResourceId",
            "evidenceResourceIds",
            "certificateResourceId",
            "certificateResourceIds",
            "reportResourceIds",
            "procedureResourceId",
        ] {
            refs.insert(key, &resource_ids);
        }
        refs.insert("bindingIds", &binding_ids);
        visit(&self.design, &refs)?;
        visit(&self.bindings, &refs)?;
        if let Some(value) = &self.actual {
            for (key, collection) in actual_mappings {
                refs.insert(key, actual.get(collection).unwrap_or(&empty));
            }
            visit(value, &refs)?;
        }
        Ok(())
    }

    fn expand_tree(&mut self) -> Result<()> {
        fn visit(
            design: &Value,
            nodes: &mut Vec<Node>,
            definition: &str,
            path: Vec<String>,
            world: Matrix,
            name: Option<&str>,
            ancestors: &mut BTreeSet<String>,
        ) -> Result<()> {
            ensure!(
                nodes.len() < 2000 && path.len() <= 64,
                "Product tree exceeds viewer expansion limits"
            );
            ensure!(
                ancestors.insert(definition.to_owned()),
                "Recursive product containment"
            );
            let product = array(design, "productDefinitions")
                .iter()
                .find(|v| text(v, "id") == definition)
                .context("Missing product definition")?;
            nodes.push(Node {
                key: if path.is_empty() {
                    "root".into()
                } else {
                    path.join("/")
                },
                name: name.unwrap_or(text(product, "name")).into(),
                definition_id: definition.into(),
                path: path.clone(),
                world,
            });
            for occurrence in array(design, "occurrences")
                .iter()
                .filter(|v| text(v, "parentDefinitionId") == definition)
            {
                let mut child = path.clone();
                child.push(text(occurrence, "id").into());
                visit(
                    design,
                    nodes,
                    text(occurrence, "childDefinitionId"),
                    child,
                    multiply(world, transform(&occurrence["transform"])?),
                    occurrence["name"].as_str(),
                    ancestors,
                )?;
            }
            ancestors.remove(definition);
            Ok(())
        }
        visit(
            &self.design,
            &mut self.nodes,
            text(&self.design, "rootDefinitionId"),
            Vec::new(),
            identity(),
            None,
            &mut BTreeSet::new(),
        )
    }

    fn check_actual(&self) -> Result<()> {
        let Some(actual) = &self.actual else {
            return Ok(());
        };
        let mut serials = BTreeSet::new();
        for subject in array(actual, "subjects") {
            let path: Vec<_> = array(subject, "occurrencePath")
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect();
            ensure!(
                self.nodes
                    .iter()
                    .any(|n| n.path == path
                        && n.definition_id == text(subject, "productDefinitionId")),
                "Physical subject has an invalid occurrence path"
            );
            if !text(subject, "serialNumber").is_empty() {
                ensure!(
                    serials.insert((
                        text(subject, "productDefinitionId"),
                        text(subject, "manufacturerActorId"),
                        text(subject, "serialNumber")
                    )),
                    "Distinct physical subjects share a serial identity"
                );
            }
        }
        for observation in array(actual, "observations") {
            let run = find(actual, "runs", text(observation, "runId"))?;
            let time = DateTime::parse_from_rfc3339(text(observation, "observedAt"))?;
            ensure!(
                time >= DateTime::parse_from_rfc3339(text(run, "startedAt"))?
                    && time <= DateTime::parse_from_rfc3339(text(run, "endedAt"))?,
                "Observation is outside its inspection run"
            );
            ensure!(
                array(run, "subjectIds")
                    .iter()
                    .any(|s| s.as_str() == Some(text(observation, "subjectId"))),
                "Observation subject is outside its run"
            );
            for calibration_id in array(run, "calibrationIds") {
                let calibration = find(
                    actual,
                    "calibrations",
                    calibration_id.as_str().unwrap_or(""),
                )?;
                ensure!(
                    text(calibration, "status") == "valid"
                        && time >= DateTime::parse_from_rfc3339(text(calibration, "calibratedAt"))?
                        && time <= DateTime::parse_from_rfc3339(text(calibration, "validUntil"))?,
                    "Calibration is not valid at the measurement date"
                );
            }
        }
        for evaluation in array(actual, "evaluations") {
            let requirement = find(
                &self.design,
                "requirements",
                text(evaluation, "requirementId"),
            )?;
            for id in array(evaluation, "observationIds") {
                let observation = find(actual, "observations", id.as_str().unwrap_or(""))?;
                ensure!(
                    text(observation, "subjectId") == text(evaluation, "subjectId")
                        && text(observation, "requirementId") == text(evaluation, "requirementId"),
                    "Evaluation refers to another subject or characteristic's observation"
                );
                let rule = find(
                    &self.design,
                    "decisionRules",
                    text(evaluation, "decisionRuleId"),
                )?;
                if text(rule, "method") == "simple-limits"
                    && text(&observation["value"], "kind") == "scalar"
                    && requirement["spec"]["limits"].is_object()
                {
                    let limits = &requirement["spec"]["limits"];
                    let quantity = &observation["value"]["quantity"];
                    let observed = Decimal::from_str(text(quantity, "value"))?
                        * decimal_scale(text(quantity, "unit"), text(limits, "unit"))?;
                    let lower = limits["lower"]
                        .as_str()
                        .map(Decimal::from_str)
                        .transpose()?;
                    let upper = limits["upper"]
                        .as_str()
                        .map(Decimal::from_str)
                        .transpose()?;
                    let pass = lower.is_none_or(|l| {
                        observed > l || observed == l && limits["lowerInclusive"] == true
                    }) && upper.is_none_or(|u| {
                        observed < u || observed == u && limits["upperInclusive"] == true
                    });
                    ensure!(
                        text(evaluation, "conformance") != (if pass { "fail" } else { "pass" }),
                        "Reported conformance contradicts a simple numeric acceptance rule"
                    );
                }
            }
            ensure!(
                text(evaluation, "disposition") != "accepted"
                    || text(evaluation, "conformance") == "pass",
                "Ordinary acceptance requires reported pass"
            );
            if text(evaluation, "disposition") == "accepted-under-deviation" {
                let deviation = find(actual, "deviations", text(evaluation, "deviationId"))?;
                let evaluated = DateTime::parse_from_rfc3339(text(evaluation, "evaluatedAt"))?;
                ensure!(
                    text(deviation, "disposition") == "use-as-is"
                        && DateTime::parse_from_rfc3339(text(deviation, "approvedAt"))?
                            <= evaluated,
                    "Deviation is not effective final acceptance"
                );
                if let Some(expiry) = deviation["expiresAt"].as_str() {
                    ensure!(
                        evaluated <= DateTime::parse_from_rfc3339(expiry)?,
                        "Deviation has expired at evaluation date"
                    );
                }
                for id in array(deviation, "approvalIds") {
                    let approval = find(actual, "approvals", id.as_str().unwrap_or(""))?;
                    ensure!(
                        DateTime::parse_from_rfc3339(text(approval, "approvedAt"))? <= evaluated,
                        "Deviation approval postdates evaluation"
                    );
                }
                ensure!(
                    array(deviation, "subjectIds")
                        .iter()
                        .any(|v| v.as_str() == Some(text(evaluation, "subjectId")))
                        && array(deviation, "requirementIds")
                            .iter()
                            .any(|v| v.as_str() == Some(text(evaluation, "requirementId")))
                        && !array(deviation, "approvalIds").is_empty(),
                    "Concession is not approved for this subject and characteristic"
                );
            }
        }
        Ok(())
    }

    /// Review the most recent recorded evaluation for one characteristic and physical subject.
    pub fn latest_evaluation(&self, requirement: &str, subject: &str) -> Option<&Value> {
        let actual = self.actual.as_ref()?;
        array(actual, "evaluations")
            .iter()
            .filter(|e| text(e, "requirementId") == requirement && text(e, "subjectId") == subject)
            .max_by_key(|e| DateTime::parse_from_rfc3339(text(e, "evaluatedAt")).ok())
    }
    /// Keep the displayed measurement tied to the displayed evaluation, rather than an older retest.
    pub fn review_observation(&self, requirement: &str, subject: &str) -> Option<&Value> {
        let actual = self.actual.as_ref()?;
        let evaluation = self.latest_evaluation(requirement, subject);
        array(actual, "observations")
            .iter()
            .filter(|o| {
                text(o, "requirementId") == requirement
                    && text(o, "subjectId") == subject
                    && evaluation.is_none_or(|e| {
                        array(e, "observationIds")
                            .iter()
                            .any(|id| id.as_str() == Some(text(o, "id")))
                    })
            })
            .max_by_key(|o| DateTime::parse_from_rfc3339(text(o, "observedAt")).ok())
    }

    pub fn resource(&self, id: &str) -> Result<(&Resource, &[u8])> {
        let resource = self.resources.get(id).context("Resource not found")?;
        Ok((resource, &self.files[&resource.path]))
    }
    pub fn root_product(&self) -> &Value {
        array(&self.design, "productDefinitions")
            .iter()
            .find(|v| text(v, "id") == text(&self.design, "rootDefinitionId"))
            .unwrap_or(&Value::Null)
    }
    pub fn report(&self) -> Value {
        serde_json::json!({"valid":true,"formatVersion":VERSION,"filename":self.filename,"packageType":self.manifest["packageType"],"checks":"archive, inventory, hashes, schemas and selected graph/actual invariants","engineeringCertification":"not-assessed","warnings":self.warnings,"resources":self.resources.len(),"productOccurrences":self.nodes.len()})
    }
}

pub fn find<'a>(v: &'a Value, collection: &str, id: &str) -> Result<&'a Value> {
    array(v, collection)
        .iter()
        .find(|item| text(item, "id") == id)
        .ok_or_else(|| anyhow!("Missing {collection} record: {id}"))
}
pub fn identity() -> Matrix {
    [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ]
}
pub fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..4).map(|k| a[i][k] * b[k][j]).sum()))
}
pub fn point(m: Matrix, p: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| m[i][0] * p[0] + m[i][1] * p[1] + m[i][2] * p[2] + m[i][3])
}
pub fn unit_scale(unit: &str) -> Result<f64> {
    Ok(match unit {
        "um" => 0.001,
        "mm" => 1.,
        "cm" => 10.,
        "m" => 1000.,
        "in" => 25.4,
        _ => bail!("Unsupported preview length unit: {unit}"),
    })
}
fn decimal_scale(source: &str, target: &str) -> Result<Decimal> {
    if source == target {
        return Ok(Decimal::ONE);
    }
    let scale = |unit| -> Result<Decimal> {
        Decimal::from_str(match unit {
            "um" => "0.001",
            "mm" => "1",
            "cm" => "10",
            "m" => "1000",
            "in" => "25.4",
            _ => bail!("Unsupported measurement unit conversion: {unit}"),
        })
        .map_err(Into::into)
    };
    Ok(scale(source)? / scale(target)?)
}
pub fn transform(value: &Value) -> Result<Matrix> {
    let rotation: Vec<f64> = array(value, "rotation")
        .iter()
        .map(|v| v.as_str().unwrap_or("").parse())
        .collect::<std::result::Result<_, _>>()?;
    let translation: Vec<f64> = array(value, "translation")
        .iter()
        .map(|v| v.as_str().unwrap_or("").parse())
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        rotation.len() == 9 && translation.len() == 3,
        "Invalid rigid transform dimensions"
    );
    ensure!(
        rotation.iter().chain(&translation).all(|v| v.is_finite()),
        "Nonfinite rigid transform"
    );
    for i in 0..3 {
        for j in 0..3 {
            let dot: f64 = (0..3)
                .map(|k| rotation[i * 3 + k] * rotation[j * 3 + k])
                .sum();
            ensure!(
                (dot - (if i == j { 1. } else { 0. })).abs() < 1e-6,
                "Nonorthogonal occurrence transform"
            );
        }
    }
    let det = rotation[0] * (rotation[4] * rotation[8] - rotation[5] * rotation[7])
        - rotation[1] * (rotation[3] * rotation[8] - rotation[5] * rotation[6])
        + rotation[2] * (rotation[3] * rotation[7] - rotation[4] * rotation[6]);
    ensure!(
        (det - 1.).abs() < 1e-6,
        "Occurrence transform reflects or scales geometry"
    );
    let scale = unit_scale(text(value, "translationUnit"))?;
    Ok([
        [
            rotation[0],
            rotation[1],
            rotation[2],
            translation[0] * scale,
        ],
        [
            rotation[3],
            rotation[4],
            rotation[5],
            translation[1] * scale,
        ],
        [
            rotation[6],
            rotation[7],
            rotation[8],
            translation[2] * scale,
        ],
        [0., 0., 0., 1.],
    ])
}
