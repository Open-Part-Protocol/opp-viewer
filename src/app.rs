// SPDX-License-Identifier: Apache-2.0
use crate::viewport::Camera;
use eframe::egui::{self, Color32, FontId, RichText, Stroke, TextStyle, Vec2};
use egui_extras::{Column, TableBuilder};
use opp_viewer::{
    geometry::Scene,
    package::{Package, array, find, text},
};
use serde_json::Value;
use std::{
    path::PathBuf,
    sync::{Arc, mpsc},
    thread,
};

const INK: Color32 = Color32::from_rgb(23, 37, 42);
const MUTED: Color32 = Color32::from_rgb(80, 103, 118);
const TEAL: Color32 = Color32::from_rgb(35, 104, 92);
const BORDER: Color32 = Color32::from_rgb(220, 229, 232);
const RED: Color32 = Color32::from_rgb(187, 41, 43);
const AMBER: Color32 = Color32::from_rgb(173, 99, 15);
const SAMPLES: [(&str, &[u8]); 5] = [
    (
        "Part design",
        include_bytes!("../fixtures/block-design.opp"),
    ),
    (
        "Part as-built",
        include_bytes!("../fixtures/block-as-built.opp"),
    ),
    (
        "Assembly design",
        include_bytes!("../fixtures/assembly-design.opp"),
    ),
    (
        "Assembly as-built",
        include_bytes!("../fixtures/assembly-as-built.opp"),
    ),
    (
        "Lot as-built",
        include_bytes!("../fixtures/lot-as-built.opp"),
    ),
];

pub struct Loaded {
    pub package: Package,
    pub scene: Scene,
}
impl Loaded {
    pub fn new(package: Package) -> Self {
        let scene = Scene::from_package(&package);
        Self { package, scene }
    }
}

pub struct Viewer {
    loaded: Option<Arc<Loaded>>,
    pending: Option<mpsc::Receiver<Result<Loaded, String>>>,
    error: Option<String>,
    selected_node: String,
    selected_subject: String,
    selected_requirement: String,
    tab: usize,
    camera: Camera,
    scan_overlay: bool,
    evidence: String,
    raw_document: String,
    detail: Option<(String, Value)>,
    compact_inspector: bool,
}

impl Viewer {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<Package>) -> Self {
        cc.egui_ctx.set_theme(egui::Theme::Light);
        let mut style = (*cc.egui_ctx.global_style()).clone();
        style.visuals = egui::Visuals::light();
        style.visuals.override_text_color = Some(INK);
        style.visuals.panel_fill = Color32::WHITE;
        style.visuals.window_fill = Color32::WHITE;
        style.visuals.selection.bg_fill = Color32::from_rgb(220, 239, 236);
        style.visuals.selection.stroke = Stroke::new(1., TEAL);
        style.spacing.item_spacing = Vec2::new(12., 10.);
        style.spacing.button_padding = Vec2::new(15., 9.);
        style.spacing.interact_size = Vec2::new(36., 34.);
        for (name, size) in [
            (TextStyle::Body, 14.),
            (TextStyle::Button, 14.),
            (TextStyle::Heading, 23.),
            (TextStyle::Small, 11.),
            (TextStyle::Monospace, 12.),
        ] {
            style.text_styles.insert(
                name.clone(),
                FontId::new(
                    size,
                    if name == TextStyle::Monospace {
                        egui::FontFamily::Monospace
                    } else {
                        egui::FontFamily::Proportional
                    },
                ),
            );
        }
        cc.egui_ctx.set_global_style(style);
        let mut viewer = Self {
            loaded: None,
            pending: None,
            error: None,
            selected_node: "root".into(),
            selected_subject: String::new(),
            selected_requirement: String::new(),
            tab: 0,
            camera: Camera::default(),
            scan_overlay: false,
            evidence: String::new(),
            raw_document: "manifest".into(),
            detail: None,
            compact_inspector: false,
        };
        if let Some(package) = initial {
            viewer.accept(Loaded::new(package));
        }
        viewer
    }
    fn accept(&mut self, loaded: Loaded) {
        self.selected_subject = loaded
            .package
            .actual
            .as_ref()
            .and_then(|a| array(a, "subjects").first())
            .map(|s| text(s, "id").into())
            .unwrap_or_default();
        self.selected_requirement = array(&loaded.package.design, "requirements")
            .iter()
            .find(|r| text(r, "id") == "req-width")
            .or_else(|| array(&loaded.package.design, "requirements").first())
            .map(|r| text(r, "id").into())
            .unwrap_or_default();
        self.selected_node = "root".into();
        self.tab = 0;
        self.camera.reset();
        self.scan_overlay = false;
        self.evidence.clear();
        self.raw_document = "manifest".into();
        self.error = None;
        self.loaded = Some(Arc::new(loaded));
    }
    fn load(&mut self, ctx: egui::Context, source: Source) {
        let (tx, rx) = mpsc::channel();
        self.pending = Some(rx);
        self.error = None;
        thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let package = match source {
                    Source::File(path) => Package::open(&path),
                    Source::Sample(index) => {
                        Package::from_bytes(SAMPLES[index].1, &format!("{}.opp", SAMPLES[index].0))
                    }
                }?;
                Ok::<_, anyhow::Error>(Loaded::new(package))
            }));
            let result = match result {
                Ok(result) => result.map_err(|e| format!("{e:#}")),
                Err(_) => Err(
                    "The package could not be reviewed safely. The previous document remains open."
                        .into(),
                ),
            };
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    fn select_node(&mut self, p: &Package, key: &str) {
        self.selected_node = key.into();
        if let Some(node) = p.nodes.iter().find(|n| n.key == key) {
            self.selected_subject = p
                .actual
                .as_ref()
                .and_then(|actual| {
                    array(actual, "subjects").iter().find(|s| {
                        array(s, "occurrencePath")
                            .iter()
                            .filter_map(Value::as_str)
                            .eq(node.path.iter().map(String::as_str))
                    })
                })
                .map(|s| text(s, "id").into())
                .unwrap_or_default();
            self.selected_requirement = array(&p.design, "requirements")
                .iter()
                .find(|r| {
                    array(r, "subjects")
                        .iter()
                        .any(|s| text(s, "productDefinitionId") == node.definition_id)
                })
                .map(|r| text(r, "id").into())
                .unwrap_or_default();
        }
    }
    fn inspector(&mut self, ui: &mut egui::Ui, p: &Package) {
        ui.label(RichText::new("Requirement inspector").strong());
        ui.separator();
        ui.add_space(9.);
        let Some(req) = array(&p.design, "requirements")
            .iter()
            .find(|r| text(r, "id") == self.selected_requirement)
        else {
            ui.label("Select a characteristic to review its definition and recorded result.");
            return;
        };
        egui::ScrollArea::vertical().id_salt("inspector_scroll").show(ui,|ui| {
            ui.heading(text(req,"name"));ui.add_space(12.);
            let observation=self.observation(p,req);
            let evaluation=self.evaluation(p,req);
            let limits=&req["spec"]["limits"];
            field(ui,"Nominal",&quantity(&req["spec"]["nominal"]),INK);
            field(ui,"Allowed",&if limits.is_object() {limits_label(limits)}else{specification(p,req)},INK);
            field(ui,"Measured",&observation.map(|o|observed(&o["value"])).unwrap_or_else(||"No recorded result".into()),if evaluation.is_some_and(|e|text(e,"conformance")=="fail"){RED}else{INK});
            field(ui,"Conformance",&evaluation.map(|e|human(text(e,"conformance"))).unwrap_or_else(||"Not evaluated".into()),if evaluation.is_some_and(|e|text(e,"conformance")=="fail"){RED}else{TEAL});
            field(ui,"Acceptance",&evaluation.map(|e|human(text(e,"disposition"))).unwrap_or_else(||"Not recorded".into()),if evaluation.is_some_and(|e|text(e,"disposition")=="accepted-under-deviation"){AMBER}else{MUTED});
            ui.add_space(8.);ui.separator();ui.add_space(8.);ui.label(RichText::new("Source geometry").color(MUTED).strong());
            let mut regions=Vec::new();
            for subject in array(req,"subjects") {
                if !text(subject,"regionId").is_empty(){regions.push(text(subject,"regionId").to_owned());}
                if let Ok(feature)=find(&p.design,"features",text(subject,"featureId")){regions.extend(array(feature,"regionIds").iter().filter_map(Value::as_str).map(str::to_owned));}
            }
            let entities:Vec<_>=array(&p.bindings,"bindings").iter().filter(|b|regions.iter().any(|r|r==text(b,"regionId"))).flat_map(|b|array(b,"entities")).map(|e|text(e,"label")).collect();
            ui.label(if entities.is_empty(){"No entity binding".into()}else{entities.join(", ")});
            ui.label(RichText::new("Face highlighting unavailable: source topology has no verified render mapping yet.").size(12.).color(MUTED));
            if let Some(e)=evaluation&& let Ok(d)=p.actual.as_ref().map(|a|find(a,"deviations",text(e,"deviationId"))).unwrap_or_else(||Err(anyhow::anyhow!("No actual"))) {
                ui.add_space(10.);ui.separator();ui.label(RichText::new("Deviation").color(AMBER).strong());ui.label(text(d,"description"));
            }
            if let Some(o)=observation&& let Some(a)=&p.actual&& let Ok(run)=find(a,"runs",text(o,"runId")) {ui.add_space(8.);ui.separator();ui.label(RichText::new("Inspection record").strong());ui.label(text(run,"method"));field(ui,"Observed at",text(o,"observedAt"),MUTED);}
            ui.add_space(10.);egui::CollapsingHeader::new("Complete requirement").show(ui,|ui|{json_preview(ui,req);});
        });
    }
    fn observation<'a>(&self, p: &'a Package, r: &Value) -> Option<&'a Value> {
        p.review_observation(text(r, "id"), &self.selected_subject)
    }
    fn evaluation<'a>(&self, p: &'a Package, r: &Value) -> Option<&'a Value> {
        p.latest_evaluation(text(r, "id"), &self.selected_subject)
    }
    fn requirements(&mut self, ui: &mut egui::Ui, p: &Package) {
        let node = p.nodes.iter().find(|n| n.key == self.selected_node);
        let rows: Vec<_> = array(&p.design, "requirements")
            .iter()
            .filter(|r| {
                self.selected_node == "root"
                    || node.is_some_and(|n| {
                        array(r, "subjects")
                            .iter()
                            .any(|s| text(s, "productDefinitionId") == n.definition_id)
                    })
            })
            .collect();
        if rows.is_empty() {
            ui.label("No declared requirements for this selection.");
            return;
        }
        TableBuilder::new(ui)
            .id_salt("requirements_table")
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .column(Column::initial(150.).at_least(110.))
            .column(Column::remainder().at_least(160.))
            .column(Column::initial(108.).at_least(90.))
            .column(Column::initial(105.).at_least(92.))
            .column(Column::initial(166.).at_least(130.))
            .header(30., |mut header| {
                for label in [
                    "Characteristic",
                    "Specification",
                    "Measured",
                    "Conformance",
                    "Acceptance",
                ] {
                    header.col(|ui| {
                        ui.label(RichText::new(label).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(42., rows.len(), |mut row| {
                    let r = rows[row.index()];
                    row.set_selected(text(r, "id") == self.selected_requirement);
                    row.col(|ui| {
                        if ui
                            .selectable_label(
                                text(r, "id") == self.selected_requirement,
                                text(r, "name"),
                            )
                            .clicked()
                        {
                            self.selected_requirement = text(r, "id").into();
                        }
                    });
                    row.col(|ui| {
                        ui.label(specification(p, r));
                    });
                    row.col(|ui| {
                        ui.label(
                            self.observation(p, r)
                                .map(|o| observed(&o["value"]))
                                .unwrap_or_else(|| "—".into()),
                        );
                    });
                    row.col(|ui| {
                        let e = self.evaluation(p, r);
                        let state = e.map(|e| text(e, "conformance")).unwrap_or("");
                        ui.label(
                            RichText::new(if state.is_empty() {
                                "—".into()
                            } else {
                                human(state)
                            })
                            .color(if state == "fail" {
                                RED
                            } else {
                                TEAL
                            }),
                        );
                    });
                    row.col(|ui| {
                        let e = self.evaluation(p, r);
                        let state = e.map(|e| text(e, "disposition")).unwrap_or("");
                        ui.label(
                            RichText::new(if state.is_empty() {
                                "—".into()
                            } else {
                                human(state)
                            })
                            .color(
                                if state == "accepted-under-deviation" {
                                    AMBER
                                } else {
                                    MUTED
                                },
                            ),
                        );
                    });
                    if row.response().clicked() {
                        self.selected_requirement = text(r, "id").into();
                    }
                });
            });
    }
    fn records(
        &mut self,
        ui: &mut egui::Ui,
        actual: &Value,
        collection: &str,
        columns: &[(&str, &str)],
    ) {
        let records = array(actual, collection);
        ui.label(RichText::new(human(collection)).strong());
        if records.is_empty() {
            ui.label("No records in this package.");
            return;
        }
        TableBuilder::new(ui)
            .id_salt(collection)
            .vscroll(false)
            .sense(egui::Sense::click())
            .striped(true)
            .resizable(true)
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .columns(Column::remainder().at_least(105.), columns.len())
            .header(28., |mut h| {
                for (label, _) in columns {
                    h.col(|ui| {
                        ui.label(RichText::new(*label).strong());
                    });
                }
            })
            .body(|body| {
                body.rows(42., records.len(), |mut row| {
                    let item = &records[row.index()];
                    for (index, (_, key)) in columns.iter().enumerate() {
                        row.col(|ui| {
                            let label = record_label(actual, key, &item[*key]);
                            if index == 0 {
                                if ui
                                    .selectable_label(false, label)
                                    .on_hover_text("Open complete record")
                                    .clicked()
                                {
                                    self.detail = Some((human(collection), item.clone()));
                                }
                            } else {
                                ui.label(label);
                            }
                        });
                    }
                    if row.response().clicked() {
                        self.detail = Some((human(collection), item.clone()));
                    }
                })
            });
        ui.label(
            RichText::new("Select a row for its complete record.")
                .size(11.)
                .color(MUTED),
        );
    }
    fn evidence(&mut self, ui: &mut egui::Ui, p: &Package) {
        egui::ScrollArea::vertical().id_salt("evidence_scroll").show(ui,|ui| {
            for resource in p.resources.values().filter(|r|!matches!(r.role.as_str(),"design"|"as-built"|"bindings"|"presentation"|"conversion-report")) {
                ui.horizontal(|ui|{if ui.selectable_label(self.evidence==resource.id,&resource.path).clicked(){self.evidence=resource.id.clone();}ui.label(RichText::new(format!("{} · {:.1} KiB",human(&resource.role),resource.size as f64/1024.)).size(12.).color(MUTED));});
            }
            if !self.evidence.is_empty()&& let Ok((resource,bytes))=p.resource(&self.evidence){ui.separator();ui.label(RichText::new(&resource.path).strong());ui.label(RichText::new(format!("SHA-256 verified · {}",resource.media_type)).size(12.).color(MUTED));
                if ui.button("Save original copy…").clicked()&& let Some(path)=rfd::FileDialog::new().set_file_name(std::path::Path::new(&resource.path).file_name().unwrap_or_default().to_string_lossy()).save_file()&& let Err(e)=std::fs::write(path,bytes){self.error=Some(format!("Could not save evidence: {e}"));}
                match std::str::from_utf8(bytes){Ok(s)=>{let clipped:String=s.chars().take(64000).collect();let mut view=clipped.as_str();ui.add(egui::TextEdit::multiline(&mut view).font(TextStyle::Monospace).desired_width(f32::INFINITY).interactive(false));if s.len()>clipped.len(){ui.label("Preview is limited; save the original for the complete content.");}},Err(_)=>{ui.label("Binary evidence is preserved. Save a copy for its specialist application.");}}
            }
        });
    }
    fn package_tab(&mut self, ui: &mut egui::Ui, p: &Package, scene: &Scene) {
        egui::ScrollArea::vertical().id_salt("package_scroll").show(ui,|ui| {
            ui.label(RichText::new("Package checks passed").color(TEAL).strong());ui.label("Archive, inventory, hashes, bundled schemas, and selected record relationships checked.");
            for warning in p.warnings.iter().chain(&scene.warnings){ui.label(RichText::new(format!("• {warning}")).size(12.).color(AMBER));}
            ui.separator();
            egui::ComboBox::from_id_salt("raw_document").selected_text(if self.raw_document=="manifest"{"Manifest"}else{&self.raw_document}).show_ui(ui,|ui|{ui.selectable_value(&mut self.raw_document,"manifest".into(),"Manifest");for id in p.documents.keys(){ui.selectable_value(&mut self.raw_document,id.clone(),id);}});
            let document=if self.raw_document=="manifest"{&p.manifest}else{p.documents.get(&self.raw_document).unwrap_or(&p.manifest)};
            json_preview(ui,document);
        });
    }
}

enum Source {
    File(PathBuf),
    Sample(usize),
}

impl eframe::App for Viewer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(rx) = &self.pending
            && let Ok(result) = rx.try_recv()
        {
            self.pending = None;
            match result {
                Ok(loaded) => self.accept(loaded),
                Err(e) => self.error = Some(e),
            }
        }
        if self.pending.is_none()
            && let Some(path) =
                ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_owned()))
        {
            self.load(ctx.clone(), Source::File(path));
        }
        let loaded = self.loaded.clone();
        egui::Panel::top("header")
            .exact_size(67.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(22, 11)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("OPP Viewer").size(28.).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                self.pending.is_none(),
                                egui::Button::new(RichText::new("Open .opp").color(Color32::WHITE))
                                    .fill(TEAL),
                            )
                            .clicked()
                            && let Some(path) = rfd::FileDialog::new()
                                .add_filter("Open Part Protocol", &["opp"])
                                .pick_file()
                        {
                            self.load(ctx.clone(), Source::File(path));
                        }
                        ui.menu_button("Examples", |ui| {
                            for (index, (name, _)) in SAMPLES.iter().enumerate() {
                                if ui.button(*name).clicked() {
                                    self.load(ctx.clone(), Source::Sample(index));
                                    ui.close();
                                }
                            }
                        });
                        if self.pending.is_some() {
                            ui.spinner();
                            ui.label("Opening package…");
                            ctx.request_repaint_after(std::time::Duration::from_millis(100));
                        }
                    });
                });
            });
        if let Some(error) = self.error.clone() {
            egui::Panel::top("error").show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(error).color(RED));
                    if ui.button("Dismiss").clicked() {
                        self.error = None;
                    }
                });
            });
        }
        egui::Panel::bottom("footer")
            .exact_size(35.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(22, 8)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if loaded.is_some() {
                            "Package checks passed · Engineering interpretation not certified"
                        } else {
                            "Local files · Read-only review"
                        })
                        .size(11.)
                        .color(MUTED),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new("OPP Viewer 0.1.0").size(11.).color(MUTED));
                    });
                });
            });
        let Some(loaded) = loaded else {
            egui::CentralPanel::default().show(ui,|ui|{ui.vertical_centered(|ui|{ui.add_space(ui.available_height()*0.3);ui.heading("Review a part's complete definition");ui.label("Open or drop an .opp file to review its geometry, requirements, and as-built records.");ui.add_space(12.);if ui.button("Try the synthetic as-built example").clicked(){self.load(ctx.clone(),Source::Sample(1));}});});
            return;
        };
        let p = &loaded.package;
        egui::Panel::top("identity")
            .exact_size(83.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .inner_margin(egui::Margin::symmetric(22, 12)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(text(p.root_product(), "name"))
                                .size(24.)
                                .strong(),
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} · Rev {} · {}",
                                text(p.root_product(), "partNumber"),
                                text(p.root_product(), "revision"),
                                human(text(&p.manifest, "packageType"))
                            ))
                            .color(MUTED),
                        );
                    });
                    if let Some(actual) = &p.actual {
                        ui.add_space(28.);
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Serial / lot").size(12.).color(MUTED));
                            let selected = array(actual, "subjects")
                                .iter()
                                .find(|s| text(s, "id") == self.selected_subject);
                            let before = self.selected_subject.clone();
                            egui::ComboBox::from_id_salt("subject")
                                .width(195.)
                                .selected_text(
                                    selected
                                        .map(subject_label)
                                        .unwrap_or_else(|| "Select physical subject".into()),
                                )
                                .show_ui(ui, |ui| {
                                    for s in array(actual, "subjects") {
                                        ui.selectable_value(
                                            &mut self.selected_subject,
                                            text(s, "id").into(),
                                            subject_label(s),
                                        );
                                    }
                                });
                            if before != self.selected_subject
                                && let Some(s) = array(actual, "subjects")
                                    .iter()
                                    .find(|s| text(s, "id") == self.selected_subject)
                            {
                                let path: Vec<_> = array(s, "occurrencePath")
                                    .iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect();
                                if let Some(node) = p.nodes.iter().find(|n| n.path == path) {
                                    let saved = self.selected_subject.clone();
                                    self.select_node(p, &node.key);
                                    self.selected_subject = saved;
                                }
                            }
                        });
                    }
                });
            });
        let compact = ui.available_width() < 1150.;
        egui::Panel::left("product_tree")
            .default_size(235.)
            .min_size(180.)
            .max_size(350.)
            .frame(
                egui::Frame::new()
                    .fill(Color32::WHITE)
                    .stroke(Stroke::new(1., BORDER))
                    .inner_margin(14),
            )
            .show(ui, |ui| {
                ui.label(RichText::new("Product structure").strong());
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for node in &p.nodes {
                        ui.horizontal(|ui| {
                            ui.add_space(node.path.len() as f32 * 14.);
                            if ui
                                .selectable_label(self.selected_node == node.key, &node.name)
                                .clicked()
                            {
                                self.select_node(p, &node.key);
                            }
                        });
                    }
                    ui.add_space(18.);
                    ui.separator();
                    if let Some(node) = p.nodes.iter().find(|n| n.key == self.selected_node) {
                        for state in array(&p.design, "states")
                            .iter()
                            .filter(|s| text(s, "productDefinitionId") == node.definition_id)
                        {
                            ui.label(RichText::new(text(state, "name")).size(12.).color(MUTED));
                        }
                        if let Ok(definition) =
                            find(&p.design, "productDefinitions", &node.definition_id)
                        {
                            ui.label(
                                RichText::new(format!(
                                    "{} · {}",
                                    text(definition, "partNumber"),
                                    human(text(definition, "kind"))
                                ))
                                .size(12.)
                                .color(MUTED),
                            );
                        }
                    }
                });
            });
        if !compact {
            egui::Panel::right("inspector")
                .default_size(300.)
                .min_size(275.)
                .max_size(400.)
                .frame(
                    egui::Frame::new()
                        .fill(Color32::WHITE)
                        .stroke(Stroke::new(1., BORDER))
                        .inner_margin(17),
                )
                .show(ui, |ui| {
                    self.inspector(ui, p);
                });
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(Color32::WHITE).inner_margin(15))
            .show(ui, |ui| {
                let canvas_height = (ui.available_height() * 0.54).clamp(190., 490.);
                egui::Frame::new()
                    .fill(Color32::from_rgb(248, 250, 251))
                    .stroke(Stroke::new(1., BORDER))
                    .corner_radius(5)
                    .inner_margin(10)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("3D view").strong());
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_enabled(
                                        !loaded.scene.scans.is_empty(),
                                        egui::Checkbox::new(
                                            &mut self.scan_overlay,
                                            "Show scan overlay",
                                        ),
                                    );
                                    if ui.button("Fit view").clicked() {
                                        self.camera.fit();
                                    }
                                    if ui.button("Reset").clicked() {
                                        self.camera.reset();
                                    }
                                },
                            );
                        });
                        self.camera.show(
                            ui,
                            &loaded.scene,
                            &self.selected_node,
                            self.scan_overlay,
                            &self.selected_subject,
                            canvas_height - 46.,
                        );
                    });
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    for (index, label) in [
                        "Requirements",
                        "Production",
                        "Equipment",
                        "Evidence",
                        "Package",
                    ]
                    .iter()
                    .enumerate()
                    {
                        ui.selectable_value(&mut self.tab, index, *label);
                    }
                    if compact && ui.button("Inspector").clicked() {
                        self.compact_inspector = true;
                    }
                });
                ui.separator();
                if self.tab == 0 {
                    self.requirements(ui, p);
                } else if self.tab == 3 {
                    self.evidence(ui, p);
                } else if self.tab == 4 {
                    self.package_tab(ui, p, &loaded.scene);
                } else if let Some(actual) = &p.actual {
                    egui::ScrollArea::vertical()
                        .id_salt("records_scroll")
                        .show(ui, |ui| {
                            if self.tab == 1 {
                                self.records(
                                    ui,
                                    actual,
                                    "productionEvents",
                                    &[
                                        ("Operation", "name"),
                                        ("Started", "startedAt"),
                                        ("Cell", "cellId"),
                                        ("Company", "organizationId"),
                                    ],
                                );
                                ui.separator();
                                self.records(
                                    ui,
                                    actual,
                                    "materialLots",
                                    &[
                                        ("Material lot", "lotNumber"),
                                        ("Heat", "heatNumber"),
                                        ("Batch", "batchNumber"),
                                    ],
                                );
                                ui.separator();
                                self.records(
                                    ui,
                                    actual,
                                    "faiReports",
                                    &[
                                        ("FAIR", "id"),
                                        ("Type", "type"),
                                        ("Recorded status", "status"),
                                    ],
                                );
                                ui.separator();
                                self.records(
                                    ui,
                                    actual,
                                    "productionCells",
                                    &[
                                        ("Cell", "name"),
                                        ("Site", "site"),
                                        ("Line", "line"),
                                        ("Company", "organizationId"),
                                    ],
                                );
                                ui.separator();
                                self.records(
                                    ui,
                                    actual,
                                    "actors",
                                    &[
                                        ("Company / person", "name"),
                                        ("Type", "kind"),
                                        ("Company", "organizationId"),
                                    ],
                                );
                            } else {
                                self.records(
                                    ui,
                                    actual,
                                    "equipment",
                                    &[
                                        ("Equipment", "model"),
                                        ("Identifier", "id"),
                                        ("Serial", "serialNumber"),
                                    ],
                                );
                                ui.separator();
                                self.records(
                                    ui,
                                    actual,
                                    "calibrations",
                                    &[
                                        ("Equipment", "equipmentId"),
                                        ("Calibrated", "calibratedAt"),
                                        ("Valid until", "validUntil"),
                                        ("Status", "status"),
                                    ],
                                );
                                ui.separator();
                                self.records(
                                    ui,
                                    actual,
                                    "runs",
                                    &[
                                        ("Inspection / test", "id"),
                                        ("Method", "method"),
                                        ("Started", "startedAt"),
                                    ],
                                );
                            }
                        });
                } else {
                    ui.label(
                        "This design package has no as-built production or equipment records.",
                    );
                }
            });
        if compact && self.compact_inspector {
            let mut open = true;
            egui::Window::new("Requirement inspector")
                .open(&mut open)
                .default_width(300.)
                .show(&ctx, |ui| {
                    self.inspector(ui, p);
                });
            self.compact_inspector = open;
        }
        if let Some((title, record)) = self.detail.clone() {
            let mut open = true;
            egui::Window::new(format!("{title} — complete record"))
                .open(&mut open)
                .default_width(620.)
                .default_height(400.)
                .show(&ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| json_preview(ui, &record));
                });
            if !open {
                self.detail = None;
            }
        }
    }
}

fn field(ui: &mut egui::Ui, label: &str, value: &str, color: Color32) {
    ui.horizontal_top(|ui| {
        ui.add_sized(
            [94., 24.],
            egui::Label::new(RichText::new(label).color(MUTED)),
        );
        ui.label(RichText::new(if value.is_empty() { "—" } else { value }).color(color));
    });
    ui.add_space(3.);
}
fn human(value: &str) -> String {
    let value = match value {
        "productionEvents" => "Production events",
        "materialLots" => "Material lots",
        "faiReports" => "First article reports",
        "productionCells" => "Production cells",
        "actors" => "Companies and people",
        "runs" => "Inspection and test runs",
        _ => value,
    };
    let value = value.replace('-', " ");
    let mut chars = value.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}
fn subject_label(s: &Value) -> String {
    for key in ["serialNumber", "lotNumber", "id"] {
        if !text(s, key).is_empty() {
            return text(s, key).into();
        }
    }
    String::new()
}
fn quantity(q: &Value) -> String {
    if text(q, "value").is_empty() {
        String::new()
    } else {
        format!("{} {}", text(q, "value"), text(q, "unit"))
    }
}
fn limits_label(l: &Value) -> String {
    let lo = text(l, "lower");
    let hi = text(l, "upper");
    if lo.is_empty() {
        format!(
            "{} {} {}",
            if l["upperInclusive"] == false {
                "<"
            } else {
                "≤"
            },
            hi,
            text(l, "unit")
        )
    } else if hi.is_empty() {
        format!(
            "{} {} {}",
            if l["lowerInclusive"] == false {
                ">"
            } else {
                "≥"
            },
            lo,
            text(l, "unit")
        )
    } else {
        format!("{lo} – {hi} {}", text(l, "unit"))
    }
}
fn observed(v: &Value) -> String {
    match text(v, "kind") {
        "scalar" => quantity(&v["quantity"]),
        "text" => text(v, "text").into(),
        "boolean" => {
            if v["boolean"] == true {
                "Yes".into()
            } else {
                "No".into()
            }
        }
        _ => value_label(v),
    }
}
fn specification(p: &Package, r: &Value) -> String {
    let spec = &r["spec"];
    if spec["limits"].is_object() {
        limits_label(&spec["limits"])
    } else if let Some(segment) = array(spec, "segments").first() {
        format!(
            "{} {}",
            human(text(segment, "characteristic")),
            quantity(&segment["tolerance"])
        )
    } else if text(r, "kind") == "material" {
        find(&p.design, "materials", text(spec, "materialDefinitionId"))
            .map(|m| text(m, "name").to_owned())
            .unwrap_or_else(|_| text(spec, "materialDefinitionId").into())
    } else {
        human(text(r, "kind"))
    }
}
fn value_label(value: &Value) -> String {
    match value {
        Value::Null => "—".into(),
        Value::String(s) => s.clone(),
        Value::Array(values) => values
            .iter()
            .map(value_label)
            .collect::<Vec<_>>()
            .join(", "),
        _ => value.to_string(),
    }
}
fn json_preview(ui: &mut egui::Ui, value: &Value) {
    let pretty = serde_json::to_string_pretty(value).unwrap_or_default();
    let clipped: String = pretty.chars().take(64000).collect();
    let mut view = clipped.as_str();
    ui.add(
        egui::TextEdit::multiline(&mut view)
            .interactive(false)
            .font(TextStyle::Monospace)
            .desired_width(f32::INFINITY),
    );
    if clipped.len() < pretty.len() {
        ui.label("Preview is limited to 64,000 characters. Full records remain preserved in the package.");
    }
}

fn record_label(actual: &Value, key: &str, value: &Value) -> String {
    let collection = match key {
        "organizationId" | "actorId" => "actors",
        "cellId" => "productionCells",
        "equipmentId" => "equipment",
        _ => return value_label(value),
    };
    value
        .as_str()
        .and_then(|id| find(actual, collection, id).ok())
        .map(|v| {
            if !text(v, "name").is_empty() {
                text(v, "name").to_owned()
            } else if !text(v, "model").is_empty() {
                text(v, "model").to_owned()
            } else {
                text(v, "id").to_owned()
            }
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| value_label(value))
}
