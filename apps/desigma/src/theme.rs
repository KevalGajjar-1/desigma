//! Figma (UI3, dark) look and feel: colors, metrics, fonts and egui style.

use egui::{Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle};

pub const PANEL_BG: Color32 = Color32::from_rgb(0x2C, 0x2C, 0x2C);
pub const INPUT_BG: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
pub const HOVER_BG: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
pub const BORDER: Color32 = Color32::from_rgb(0x44, 0x44, 0x44);
pub const TEXT: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const TEXT_SECONDARY: Color32 = Color32::from_rgb(0xB3, 0xB3, 0xB3);
pub const TEXT_TERTIARY: Color32 = Color32::from_rgb(0x75, 0x75, 0x75);
pub const BLUE: Color32 = Color32::from_rgb(0x0D, 0x99, 0xFF);
pub const SELECTED_ROW: Color32 = Color32::from_rgb(0x2E, 0x45, 0x5A);
pub const SELECTED_CHILD_ROW: Color32 = Color32::from_rgb(0x2D, 0x37, 0x40);
pub const FRAME_LABEL: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);
/// Purple used for component-ish accents; kept for parity with Figma's palette.
pub const PURPLE: Color32 = Color32::from_rgb(0x97, 0x47, 0xFF);

pub const PANEL_WIDTH: f32 = 240.0;
pub const ROW_HEIGHT: f32 = 32.0;
pub const LAYER_ROW_HEIGHT: f32 = 32.0;
pub const INPUT_HEIGHT: f32 = 24.0;
pub const FONT_SIZE: f32 = 11.0;
pub const RADIUS: u8 = 5;

pub const MEDIUM: &str = "Inter Medium";
pub const SEMIBOLD: &str = "Inter SemiBold";

pub fn font(size: f32) -> FontId {
    FontId::proportional(size)
}

pub fn font_medium(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(MEDIUM.into()))
}

pub fn font_semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Font family matching a CSS weight, so on-canvas text editing uses the same face as the renderer.
pub fn family_for_weight(weight: u16) -> FontFamily {
    match weight {
        0..=449 => FontFamily::Proportional,
        450..=549 => FontFamily::Name(MEDIUM.into()),
        _ => FontFamily::Name(SEMIBOLD.into()),
    }
}

pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        FontData::from_static(desigma_render::inter_regular_bytes()).into(),
    );
    fonts.font_data.insert(
        MEDIUM.into(),
        FontData::from_static(desigma_render::inter_medium_bytes()).into(),
    );
    fonts.font_data.insert(
        SEMIBOLD.into(),
        FontData::from_static(desigma_render::inter_semibold_bytes()).into(),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    for name in [MEDIUM, SEMIBOLD] {
        let mut fam = vec![name.to_string()];
        fam.extend(fallback.iter().skip(1).cloned());
        fonts.families.insert(FontFamily::Name(name.into()), fam);
    }
    ctx.set_fonts(fonts);

    ctx.global_style_mut(|style| {
        style.text_styles = [
            (TextStyle::Small, font(10.0)),
            (TextStyle::Body, font(FONT_SIZE)),
            (TextStyle::Button, font(FONT_SIZE)),
            (TextStyle::Heading, font_semibold(FONT_SIZE)),
            (TextStyle::Monospace, FontId::monospace(FONT_SIZE)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 8.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
        style.spacing.interact_size = egui::vec2(24.0, INPUT_HEIGHT);
        style.spacing.window_margin = Margin::same(12);
        style.spacing.menu_margin = Margin::same(8);
        style.interaction.selectable_labels = false;

        let v = &mut style.visuals;
        *v = egui::Visuals::dark();
        v.panel_fill = PANEL_BG;
        v.window_fill = PANEL_BG;
        v.extreme_bg_color = INPUT_BG;
        v.faint_bg_color = HOVER_BG;
        v.override_text_color = Some(TEXT);
        v.window_stroke = Stroke::new(1.0, BORDER);
        v.window_corner_radius = CornerRadius::same(13);
        v.menu_corner_radius = CornerRadius::same(13);
        v.selection.bg_fill = BLUE.linear_multiply(0.4);
        v.selection.stroke = Stroke::new(1.0, BLUE);
        v.hyperlink_color = BLUE;
        v.text_cursor.stroke = Stroke::new(1.0, BLUE);

        let r = CornerRadius::same(RADIUS);
        v.widgets.noninteractive.bg_fill = PANEL_BG;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.noninteractive.corner_radius = r;
        for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            w.corner_radius = r;
            w.expansion = 0.0;
        }
        v.widgets.inactive.bg_fill = INPUT_BG;
        v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
        v.widgets.inactive.bg_stroke = Stroke::NONE;
        v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.hovered.bg_fill = INPUT_BG;
        v.widgets.hovered.weak_bg_fill = HOVER_BG;
        v.widgets.hovered.bg_stroke = Stroke::new(1.0, BORDER);
        v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.active.bg_fill = INPUT_BG;
        v.widgets.active.weak_bg_fill = HOVER_BG;
        v.widgets.active.bg_stroke = Stroke::new(1.0, BLUE);
        v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
        v.widgets.open.weak_bg_fill = HOVER_BG;
    });
}

pub fn to_color32(c: desigma_core::Color) -> Color32 {
    let [r, g, b, a] = c.to_rgba8();
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

pub fn from_color32(c: Color32) -> desigma_core::Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    desigma_core::Color::from_rgb8(r, g, b).with_alpha(a as f32 / 255.0)
}
