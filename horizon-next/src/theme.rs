use eframe::egui::{self, Color32, Rounding, Stroke, Vec2};

pub const BG: Color32 = Color32::from_rgb(11, 13, 18);
pub const SIDEBAR: Color32 = Color32::from_rgb(16, 19, 26);
pub const CARD: Color32 = Color32::from_rgb(22, 26, 35);
pub const CARD_HOVER: Color32 = Color32::from_rgb(30, 35, 47);
pub const BORDER: Color32 = Color32::from_rgb(38, 44, 58);
pub const ACCENT: Color32 = Color32::from_rgb(52, 224, 161);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(24, 92, 74);
pub const TEXT: Color32 = Color32::from_rgb(232, 236, 244);
pub const MUTED: Color32 = Color32::from_rgb(130, 140, 160);
pub const DANGER: Color32 = Color32::from_rgb(240, 100, 100);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = CARD;
    v.extreme_bg_color = Color32::from_rgb(14, 17, 23);
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = ACCENT_DIM;
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);

    let r = Rounding::same(10.0);
    v.widgets.inactive.rounding = r;
    v.widgets.hovered.rounding = r;
    v.widgets.active.rounding = r;
    v.widgets.open.rounding = r;
    v.widgets.inactive.weak_bg_fill = CARD_HOVER;
    v.widgets.inactive.bg_fill = CARD_HOVER;
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(40, 47, 62);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT_DIM);
    v.widgets.active.weak_bg_fill = ACCENT_DIM;
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(10.0, 12.0);
        s.spacing.button_padding = Vec2::new(16.0, 9.0);
        s.spacing.interact_size.y = 34.0;
    });
}
