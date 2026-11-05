#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod auth;
mod config;
mod launcher;
mod theme;

use config::{Config, Instance};
use eframe::egui::{self, Align2, Color32, FontId, Margin, RichText, Rounding, Sense, Stroke, Vec2};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use theme::*;

type VersionList = Arc<Mutex<Option<Result<Vec<launcher::VersionEntry>, String>>>>;

#[derive(PartialEq, Clone, Copy)]
enum Page {
    Play,
    Instances,
    Settings,
    About,
}

struct App {
    root: PathBuf,
    cfg: Config,
    page: Page,
    progress: launcher::Shared,
    versions: VersionList,
    new_name: String,
    new_version: String,
    new_ram: u32,
    show_snapshots: bool,
    login: Arc<Mutex<auth::LoginState>>,
    account: Option<auth::Stored>,
}

impl App {
    fn new() -> Self {
        let root = config::data_root();
        let cfg = Config::load(&root);
        let account = auth::load(&root);
        let versions: VersionList = Arc::new(Mutex::new(None));
        let v2 = versions.clone();
        thread::spawn(move || {
            *v2.lock().unwrap() = Some(launcher::fetch_versions());
        });
        Self {
            root,
            cfg,
            page: Page::Play,
            progress: Default::default(),
            versions,
            new_name: String::new(),
            new_version: "1.21.1".into(),
            new_ram: 4096,
            show_snapshots: false,
            login: Default::default(),
            account,
        }
    }

    fn launch(&mut self) {
        let Some(inst) = self.cfg.instances.get(self.cfg.selected).cloned() else {
            return;
        };
        {
            let mut g = self.progress.lock().unwrap();
            if g.running {
                return;
            }
            *g = launcher::Progress {
                running: true,
                message: "Starting…".into(),
                ..Default::default()
            };
        }
        self.cfg.save(&self.root);
        let (p, root) = (self.progress.clone(), self.root.clone());
        let (user, java) = (self.cfg.username.clone(), self.cfg.java_path.clone());
        let (cid, signed_in) = (self.cfg.ms_client_id.clone(), self.account.is_some());
        thread::spawn(move || {
            let session = if signed_in {
                p.lock().unwrap().message = "Signing in…".into();
                auth::refresh_session(&root, &cid)
            } else {
                Ok(auth::Session::offline(&user))
            };
            let r = session.and_then(|s| launcher::install_and_launch(&root, &inst, &s, &java, &p));
            let mut g = p.lock().unwrap();
            g.running = false;
            if let Err(e) = r {
                g.error = Some(e);
            }
        });
    }

    fn version_ids(&self) -> Vec<String> {
        match &*self.versions.lock().unwrap() {
            Some(Ok(v)) => v
                .iter()
                .filter(|x| x.kind == "release" || (self.show_snapshots && x.kind == "snapshot"))
                .map(|x| x.id.clone())
                .collect(),
            _ => vec![],
        }
    }

    // ───────────────────────── sidebar ─────────────────────────
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
            ui.painter().rect_filled(rect, Rounding::same(9.0), ACCENT);
            ui.painter().rect_filled(rect.shrink(9.0), Rounding::same(3.0), SIDEBAR);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new("HORIZON").size(16.0).strong());
                ui.label(RichText::new("NEXT").size(11.0).color(ACCENT).strong());
            });
        });
        ui.add_space(26.0);

        for (page, icon, label) in [
            (Page::Play, "▶", "Play"),
            (Page::Instances, "☰", "Instances"),
            (Page::Settings, "⚙", "Settings"),
            (Page::About, "✦", "About"),
        ] {
            if nav_item(ui, icon, label, self.page == page).clicked() {
                self.page = page;
            }
        }

        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(MUTED).size(12.0));
            match &self.account {
                Some(a) => {
                    ui.label(RichText::new(a.name.clone()).strong());
                    ui.label(RichText::new("MICROSOFT ACCOUNT").color(ACCENT).size(10.0));
                }
                None => {
                    ui.label(RichText::new(self.cfg.username.clone()).strong());
                    ui.label(RichText::new("OFFLINE").color(MUTED).size(10.0));
                }
            }
        });
    }

    // ───────────────────────── play ─────────────────────────
    fn play_page(&mut self, ui: &mut egui::Ui) {
        // Hero banner
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 250.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, Rounding::same(20.0), Color32::from_rgb(14, 34, 38));
        painter.circle_filled(rect.right_top() + Vec2::new(-80.0, 70.0), 150.0, rgba(52, 224, 161, 38));
        painter.circle_filled(rect.right_top() + Vec2::new(-80.0, 70.0), 90.0, rgba(52, 224, 161, 46));
        painter.circle_filled(rect.left_bottom() + Vec2::new(120.0, 20.0), 130.0, rgba(80, 140, 255, 30));
        // pixel "terrain" along the bottom
        let px = 22.0;
        let cols = (rect.width() / px).ceil() as i32;
        for i in 0..cols {
            let h = 1 + ((i * 7 + (i * i) % 5) % 4);
            for j in 0..h {
                let x = rect.left() + i as f32 * px;
                let y = rect.bottom() - (j as f32 + 1.0) * px;
                let shade = if j == h - 1 { rgba(52, 224, 161, 70) } else { rgba(255, 255, 255, 10 + (i % 3) as u8 * 4) };
                painter.rect_filled(
                    egui::Rect::from_min_size(egui::pos2(x, y), Vec2::splat(px - 2.0)).intersect(rect),
                    Rounding::same(3.0),
                    shade,
                );
            }
        }
        painter.rect_stroke(rect, Rounding::same(20.0), Stroke::new(1.0, BORDER));
        let current = self.cfg.instances.get(self.cfg.selected);
        painter.text(
            rect.left_top() + Vec2::new(32.0, 40.0),
            Align2::LEFT_TOP,
            "READY TO PLAY",
            FontId::proportional(12.0),
            ACCENT,
        );
        painter.text(
            rect.left_top() + Vec2::new(32.0, 62.0),
            Align2::LEFT_TOP,
            current.map(|i| i.name.as_str()).unwrap_or("No instance"),
            FontId::proportional(40.0),
            TEXT,
        );
        painter.text(
            rect.left_top() + Vec2::new(32.0, 114.0),
            Align2::LEFT_TOP,
            current.map(|i| format!("Minecraft {}  ·  {} MB", i.version, i.ram_mb)).unwrap_or_default(),
            FontId::proportional(16.0),
            MUTED,
        );

        ui.add_space(6.0);
        let prog = self.progress.lock().unwrap().clone();
        card(ui, |ui| {
            ui.horizontal(|ui| {
                let mut chosen = self.cfg.selected;
                ui.vertical(|ui| {
                    ui.label(RichText::new("INSTANCE").size(11.0).color(MUTED));
                    let label = self
                        .cfg
                        .instances
                        .get(chosen)
                        .map(|i| i.name.clone())
                        .unwrap_or_else(|| "—".into());
                    egui::ComboBox::from_id_salt("inst_pick").width(220.0).selected_text(label).show_ui(ui, |ui| {
                        for (i, inst) in self.cfg.instances.iter().enumerate() {
                            ui.selectable_value(&mut chosen, i, &inst.name);
                        }
                    });
                });
                if chosen != self.cfg.selected {
                    self.cfg.selected = chosen;
                    self.cfg.save(&self.root);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if prog.running { "Launching…" } else { "▶  PLAY" };
                    let btn = egui::Button::new(RichText::new(label).size(20.0).strong().color(Color32::BLACK))
                        .fill(ACCENT)
                        .rounding(Rounding::same(14.0))
                        .min_size(Vec2::new(220.0, 56.0));
                    if ui.add_enabled(!prog.running, btn).clicked() {
                        self.launch();
                    }
                });
            });
            if prog.running || prog.fraction > 0.0 || prog.error.is_some() {
                ui.add_space(4.0);
                if let Some(e) = &prog.error {
                    ui.label(RichText::new(e).color(DANGER));
                } else {
                    ui.label(RichText::new(&prog.message).color(MUTED));
                    ui.add(egui::ProgressBar::new(prog.fraction).fill(ACCENT).rounding(Rounding::same(8.0)));
                }
            }
        });
    }

    // ───────────────────────── instances ─────────────────────────
    fn instances_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Instances", "Separate game profiles with their own version and memory.");

        card(ui, |ui| {
            ui.label(RichText::new("NEW INSTANCE").size(11.0).color(MUTED));
            let ids = self.version_ids();
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.new_name).hint_text("Name").desired_width(170.0));
                egui::ComboBox::from_id_salt("new_ver").width(110.0).selected_text(self.new_version.clone()).show_ui(ui, |ui| {
                    for id in &ids {
                        ui.selectable_value(&mut self.new_version, id.clone(), id);
                    }
                });
                ui.add(egui::DragValue::new(&mut self.new_ram).range(512..=32768).speed(64).suffix(" MB"));
                ui.checkbox(&mut self.show_snapshots, "Snapshots");
                let ok = !self.new_name.trim().is_empty()
                    && !self.cfg.instances.iter().any(|i| i.name == self.new_name.trim());
                let add = egui::Button::new(RichText::new("+ Add").strong().color(Color32::BLACK)).fill(ACCENT);
                if ui.add_enabled(ok, add).clicked() {
                    self.cfg.instances.push(Instance {
                        name: self.new_name.trim().to_string(),
                        version: self.new_version.clone(),
                        ram_mb: self.new_ram,
                    });
                    self.new_name.clear();
                    self.cfg.save(&self.root);
                }
            });
            match &*self.versions.lock().unwrap() {
                None => { ui.label(RichText::new("Loading version list…").color(MUTED)); }
                Some(Err(e)) => { ui.label(RichText::new(e).color(DANGER)); }
                _ => {}
            }
        });

        let mut select = None;
        let mut delete = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, inst) in self.cfg.instances.iter().enumerate() {
                let selected = i == self.cfg.selected;
                egui::Frame::none()
                    .fill(CARD)
                    .rounding(Rounding::same(14.0))
                    .stroke(Stroke::new(1.0, if selected { ACCENT_DIM } else { BORDER }))
                    .inner_margin(Margin::same(16.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.label(RichText::new(&inst.name).size(18.0).strong());
                                ui.label(RichText::new(format!("Minecraft {} · {} MB", inst.version, inst.ram_mb)).color(MUTED));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.add_enabled(self.cfg.instances.len() > 1, egui::Button::new(RichText::new("Delete").color(DANGER))).clicked() {
                                    delete = Some(i);
                                }
                                if selected {
                                    ui.label(RichText::new("● SELECTED").color(ACCENT).size(12.0));
                                } else if ui.button("Select").clicked() {
                                    select = Some(i);
                                }
                            });
                        });
                    });
            }
        });
        if let Some(i) = select {
            self.cfg.selected = i;
            self.cfg.save(&self.root);
        }
        if let Some(i) = delete {
            self.cfg.instances.remove(i);
            self.cfg.selected = self.cfg.selected.min(self.cfg.instances.len() - 1);
            self.cfg.save(&self.root);
        }
    }

    // ───────────────────────── settings ─────────────────────────
    fn settings_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "Settings", "Account and Java runtime.");
        let mut changed = false;
        let mut sign_out = false;
        card(ui, |ui| {
            ui.label(RichText::new("MICROSOFT ACCOUNT").size(11.0).color(MUTED));
            let state = self.login.lock().unwrap().clone();
            match (&self.account, &state) {
                (_, auth::LoginState::Code { code, url }) => {
                    ui.label("Open this page and enter the code:");
                    ui.hyperlink_to(url, url);
                    ui.label(RichText::new(code).size(30.0).strong().color(ACCENT));
                    if ui.button("Copy code").clicked() {
                        ui.ctx().copy_text(code.clone());
                    }
                    ui.label(RichText::new("Waiting for you to finish signing in…").color(MUTED));
                }
                (_, auth::LoginState::Working) => {
                    ui.label("Signing in to Xbox and Minecraft…");
                }
                (Some(acc), _) => {
                    ui.label(RichText::new(&acc.name).size(18.0).strong());
                    ui.label(RichText::new("Signed in with Microsoft").color(ACCENT));
                    if ui.button("Sign out").clicked() {
                        sign_out = true;
                    }
                }
                (None, _) => {
                    if let auth::LoginState::Failed(e) = &state {
                        ui.label(RichText::new(e).color(DANGER));
                    }
                    let btn = egui::Button::new(RichText::new("Sign in with Microsoft").strong().color(Color32::BLACK)).fill(ACCENT);
                    if ui.add(btn).clicked() {
                        auth::start_login(self.root.clone(), self.cfg.ms_client_id.clone(), self.login.clone());
                    }
                }
            }
        });
        if sign_out {
            auth::clear(&self.root);
            self.account = None;
        }
        card(ui, |ui| {
            ui.label(RichText::new("OFFLINE USERNAME (used when not signed in)").size(11.0).color(MUTED));
            changed |= ui.add(egui::TextEdit::singleline(&mut self.cfg.username).desired_width(320.0)).changed();
            ui.label(RichText::new("JAVA EXECUTABLE").size(11.0).color(MUTED));
            changed |= ui.add(egui::TextEdit::singleline(&mut self.cfg.java_path).desired_width(320.0)).changed();
            ui.label(RichText::new("Use a full path if `java` is not on your PATH. Modern versions need Java 17 or 21.").color(MUTED).size(12.0));
            ui.label(RichText::new("AZURE CLIENT ID (for Microsoft sign-in)").size(11.0).color(MUTED));
            changed |= ui.add(egui::TextEdit::singleline(&mut self.cfg.ms_client_id).desired_width(320.0)).changed();
        });
        card(ui, |ui| {
            ui.label(RichText::new("DATA FOLDER").size(11.0).color(MUTED));
            ui.label(self.root.display().to_string());
        });
        if changed {
            self.cfg.save(&self.root);
        }
    }

    fn about_page(&mut self, ui: &mut egui::Ui) {
        heading(ui, "About", "Horizon Next — a fresh take on a Minecraft launcher.");
        card(ui, |ui| {
            ui.label("Built in Rust with egui. Downloads Minecraft directly from Mojang's servers.");
            ui.label(RichText::new("Sign in with a Microsoft account that owns Minecraft: Java Edition, or play offline with a custom name. Please follow Mojang's EULA.").color(MUTED));
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        let login_busy = matches!(*self.login.lock().unwrap(), auth::LoginState::Code { .. } | auth::LoginState::Working);
        if self.progress.lock().unwrap().running || login_busy {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        let done = matches!(*self.login.lock().unwrap(), auth::LoginState::Done);
        if done {
            *self.login.lock().unwrap() = auth::LoginState::Idle;
            self.account = auth::load(&self.root);
        }
        egui::SidePanel::left("nav")
            .exact_width(230.0)
            .resizable(false)
            .frame(egui::Frame::none().fill(SIDEBAR).inner_margin(Margin::same(18.0)))
            .show(ctx, |ui| self.sidebar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(BG).inner_margin(Margin::same(28.0)))
            .show(ctx, |ui| match self.page {
                Page::Play => self.play_page(ui),
                Page::Instances => self.instances_page(ui),
                Page::Settings => self.settings_page(ui),
                Page::About => self.about_page(ui),
            });
    }
}

// ───────────────────────── helpers ─────────────────────────
fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn heading(ui: &mut egui::Ui, title: &str, sub: &str) {
    ui.label(RichText::new(title).size(30.0).strong());
    ui.label(RichText::new(sub).color(MUTED));
    ui.add_space(6.0);
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(CARD)
        .rounding(Rounding::same(16.0))
        .stroke(Stroke::new(1.0, BORDER))
        .inner_margin(Margin::same(18.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn nav_item(ui: &mut egui::Ui, icon: &str, label: &str, selected: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::click());
    let bg = if selected {
        rgba(52, 224, 161, 28)
    } else if resp.hovered() {
        CARD_HOVER
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, Rounding::same(10.0), bg);
    if selected {
        let bar = egui::Rect::from_min_size(rect.left_top() + Vec2::new(0.0, 9.0), Vec2::new(3.0, 24.0));
        ui.painter().rect_filled(bar, Rounding::same(2.0), ACCENT);
    }
    let color = if selected { ACCENT } else { TEXT };
    ui.painter().text(rect.left_center() + Vec2::new(16.0, 0.0), Align2::LEFT_CENTER, icon, FontId::proportional(15.0), color);
    ui.painter().text(rect.left_center() + Vec2::new(44.0, 0.0), Align2::LEFT_CENTER, label, FontId::proportional(15.0), color);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1040.0, 660.0])
            .with_min_inner_size([860.0, 560.0])
            .with_title("Horizon Next"),
        ..Default::default()
    };
    eframe::run_native(
        "Horizon Next",
        options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}
