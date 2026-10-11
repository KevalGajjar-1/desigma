//! Every visual constant of the editor chrome lives here (mirrors
//! `assets/ui-spec.json`, measured from the real Figma UI3 dark theme).
//! Nothing outside this module may hardcode colors, sizes, radii or fonts.

use egui::{Color32, CornerRadius, FontFamily, FontId};

/// Opaque color from `0xRRGGBB`.
pub const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Color from `0xRRGGBB` plus alpha 0..=255 (premultiplied internally).
pub const fn rgba(c: u32, a: u8) -> Color32 {
    let r = ((c >> 16) & 0xFF) * a as u32 / 255;
    let g = ((c >> 8) & 0xFF) * a as u32 / 255;
    let b = (c & 0xFF) * a as u32 / 255;
    Color32::from_rgba_premultiplied(r as u8, g as u8, b as u8, a)
}

// ---------------------------------------------------------------- colors
pub const BG: Color32 = rgb(0x2C2C2C);
pub const BG_SECONDARY: Color32 = rgb(0x383838);
pub const BG_TERTIARY: Color32 = rgb(0x444444);
pub const BG_HOVER: Color32 = rgb(0x383838);
pub const BG_PRESSED: Color32 = rgb(0x444444);
pub const BG_MENU: Color32 = rgb(0x1E1E1E);
pub const BG_SELECTED: Color32 = rgb(0x394360);
pub const BG_SELECTED_HOVER: Color32 = rgb(0x4A5878);
pub const BG_SELECTED_SECONDARY: Color32 = rgb(0x32394D);
pub const BG_BRAND: Color32 = rgb(0x0C8CE9);
pub const BG_BRAND_HOVER: Color32 = rgb(0x0A6DC2);
pub const BG_BRAND_PRESSED: Color32 = rgb(0x105CAD);
pub const BG_TOOLBAR: Color32 = rgb(0x2C2C2C);
pub const BG_TOOLBAR_HOVER: Color32 = rgb(0x111111);
pub const BG_TOOLBAR_SELECTED: Color32 = rgb(0x0C8CE9);
pub const BG_TOOLBAR_MODE_SWITCHER: Color32 = rgb(0x444444);
pub const BG_SELECTED_AREA: Color32 = rgba(0x0D99FF, 0x33);
pub const BORDER: Color32 = rgb(0x444444);
pub const BORDER_MENU: Color32 = rgb(0x383838);
pub const BORDER_SELECTED: Color32 = rgb(0x0C8CE9);
pub const BORDER_TRANSLUCENT: Color32 = rgba(0xFFFFFF, 0x1A);
pub const TEXT: Color32 = rgb(0xFFFFFF);
pub const TEXT_SECONDARY: Color32 = rgba(0xFFFFFF, 0xB2);
pub const TEXT_TERTIARY: Color32 = rgba(0xFFFFFF, 0x66);
pub const TEXT_DISABLED: Color32 = rgba(0xFFFFFF, 0x66);
pub const TEXT_BRAND: Color32 = rgb(0x7CC4F8);
pub const TEXT_COMPONENT: Color32 = rgb(0xD1A8FF);
pub const ICON: Color32 = rgb(0xFFFFFF);
pub const ICON_SECONDARY: Color32 = rgba(0xFFFFFF, 0xB2);
pub const ICON_TERTIARY: Color32 = rgba(0xFFFFFF, 0x66);
pub const ICON_SELECTED: Color32 = rgb(0x7CC4F8);
pub const DANGER: Color32 = rgb(0xF24822);
pub const SHADOW_COLOR: Color32 = rgba(0x000000, 0x59);
pub const SHADOW_COLOR_STRONG: Color32 = rgba(0x000000, 0x80);
pub const INNER_HIGHLIGHT: Color32 = rgba(0xFFFFFF, 0x14);
pub const LOGO_GRADIENT_A: Color32 = rgb(0x0C8CE9);
pub const AVATAR_BG: Color32 = rgb(0xE6A23C);
pub const AVATAR_FG: Color32 = rgb(0x1E1E1E);
pub const BADGE_BG: Color32 = rgb(0x383838);
pub const CHECKER_A: Color32 = rgb(0xFFFFFF);
pub const CHECKER_B: Color32 = rgb(0xCCCCCC);

// canvas overlays
pub const CANVAS_SELECTION: Color32 = rgb(0x0D99FF);
pub const CANVAS_COMPONENT: Color32 = rgb(0x9747FF);
pub const CANVAS_HANDLE_FILL: Color32 = rgb(0xFFFFFF);
pub const CANVAS_SIZE_BADGE_TEXT: Color32 = rgb(0xFFFFFF);
pub const CANVAS_LABEL: Color32 = rgba(0x000000, 0x80);
pub const CANVAS_LABEL_DARK: Color32 = rgba(0xFFFFFF, 0x99);
pub const CANVAS_MARQUEE_FILL: Color32 = BG_SELECTED_AREA;
pub const GUIDE_COLOR: Color32 = rgb(0xF24E7B);
pub const GUIDE_TEXT: Color32 = rgb(0xFFFFFF);
pub const PEN_POINT: Color32 = rgb(0xFFFFFF);
pub const DROP_INDICATOR: Color32 = rgb(0x0C8CE9);
pub const LIGHT_BG_THRESHOLD: f32 = 0.5;

// ---------------------------------------------------------------- fonts
pub const FONT_SIZE_SMALL: f32 = 9.0;
pub const FONT_SIZE_BODY: f32 = 11.0;
pub const FONT_SIZE_LARGE: f32 = 13.0;
pub const FONT_SIZE_LABEL_CANVAS: f32 = 11.0;
pub const FAMILY_STRONG: &str = "desigma-strong";
pub const UI_FONT_FAMILY: &str = "Inter";
/// Variable-font instances of the bundled Inter (ui-spec font.weight_default / weight_strong).
pub const UI_FONT_WEIGHT: f32 = 450.0;
pub const UI_FONT_WEIGHT_STRONG: f32 = 550.0;
/// Inter's optical-size axis at text sizes.
pub const UI_FONT_OPSZ: f32 = 14.0;

pub fn font_small() -> FontId {
    FontId::new(FONT_SIZE_SMALL, FontFamily::Proportional)
}
pub fn font_small_strong() -> FontId {
    FontId::new(FONT_SIZE_SMALL, FontFamily::Name(FAMILY_STRONG.into()))
}
pub fn font_body() -> FontId {
    FontId::new(FONT_SIZE_BODY, FontFamily::Proportional)
}
pub fn font_body_strong() -> FontId {
    FontId::new(FONT_SIZE_BODY, FontFamily::Name(FAMILY_STRONG.into()))
}
pub fn font_large() -> FontId {
    FontId::new(FONT_SIZE_LARGE, FontFamily::Proportional)
}
pub fn font_large_strong() -> FontId {
    FontId::new(FONT_SIZE_LARGE, FontFamily::Name(FAMILY_STRONG.into()))
}
pub fn font_mono() -> FontId {
    FontId::new(FONT_SIZE_BODY, FontFamily::Monospace)
}

// ---------------------------------------------------------------- radii
pub const RADIUS_SMALL: u8 = 2;
pub const RADIUS_MEDIUM: u8 = 5;
pub const RADIUS_LARGE: u8 = 13;
pub const RADIUS_FULL: u8 = 255;

pub fn cr(r: u8) -> CornerRadius {
    CornerRadius::same(r)
}
pub fn cr_each(nw: u8, ne: u8, se: u8, sw: u8) -> CornerRadius {
    CornerRadius { nw, ne, se, sw }
}

// ---------------------------------------------------------------- spacing

// ---------------------------------------------------------------- shell layout
pub const RAIL_WIDTH: f32 = 56.0;
pub const BORDER_WIDTH: f32 = 1.0;
pub const RAIL_MENU_BUTTON_X: f32 = 12.0;
pub const RAIL_MENU_BUTTON_Y: f32 = 8.0;
pub const RAIL_MENU_BUTTON_SIZE: f32 = 32.0;
pub const RAIL_ITEM_SIZE: f32 = 56.0;
pub const RAIL_FIRST_Y: f32 = 56.0;
pub const RAIL_GROUP_GAP: f32 = 16.0;
pub const RAIL_ICON_SIZE: f32 = 20.0;
pub const RAIL_LABEL_GAP: f32 = 4.0;

pub const LEFT_PANEL_WIDTH: f32 = 240.0;
pub const LEFT_HEADER_HEIGHT: f32 = 65.0;
pub const LEFT_FILE_NAME_Y: f32 = 14.0;
pub const LEFT_SUBLABEL_Y: f32 = 36.0;
pub const PANEL_TITLE_ROW_HEIGHT: f32 = 40.0;
pub const PANEL_ACTION_BUTTON: f32 = 24.0;
pub const PANEL_ACTION_RIGHT_PAD: f32 = 8.0;
pub const PANEL_PADDING_X: f32 = 16.0;
pub const PAGE_ROW_HEIGHT: f32 = 24.0;
pub const PAGE_ROW_INSET: f32 = 8.0;
pub const PAGE_ROW_TEXT_PAD: f32 = 8.0;
pub const PAGES_MAX_VISIBLE_ROWS: usize = 5;
pub const PAGE_MENU_WIDTH: f32 = 160.0;
pub const LAYER_HEADER_HEIGHT: f32 = 40.0;
pub const LAYER_ROW_HEIGHT: f32 = 32.0;
pub const LAYER_INDENT: f32 = 16.0;
pub const LAYER_CHEVRON_BOX: f32 = 24.0;
pub const LAYER_CHEVRON_PAD: f32 = 8.0;
pub const LAYER_ICON_SIZE: f32 = 16.0;
pub const LAYER_ICON_GAP: f32 = 0.0;
pub const LAYER_TOGGLE_SIZE: f32 = 24.0;
pub const LAYER_DROP_THICKNESS: f32 = 2.0;
pub const BADGE_HEIGHT: f32 = 16.0;
pub const BADGE_PAD_X: f32 = 5.0;

pub const RIGHT_PANEL_WIDTH: f32 = 240.0;
pub const RIGHT_AVATAR_X: f32 = 9.0;
pub const RIGHT_AVATAR_Y: f32 = 12.0;
pub const RIGHT_AVATAR_W: f32 = 50.0;
pub const RIGHT_AVATAR_H: f32 = 24.0;
pub const RIGHT_PRESENT_FROM_RIGHT: f32 = 116.0;
pub const RIGHT_BTN_Y: f32 = 8.0;
pub const RIGHT_BTN_SIZE: f32 = 32.0;
pub const RIGHT_PRESENT_CHEVRON_W: f32 = 16.0;
pub const RIGHT_SHARE_FROM_RIGHT: f32 = 63.0;
pub const RIGHT_SHARE_W: f32 = 55.0;
pub const RIGHT_TABS_Y: f32 = 48.0;
pub const RIGHT_TABS_H: f32 = 24.0;
pub const RIGHT_TABS_X: f32 = 9.0;
pub const RIGHT_TAB_PAD_X: f32 = 8.0;
pub const RIGHT_ZOOM_FROM_RIGHT: f32 = 63.0;
pub const RIGHT_ZOOM_W: f32 = 55.0;
pub const RIGHT_CONTENT_TOP: f32 = 81.0;
pub const RIGHT_SELECTION_HEADER_Y: f32 = 93.0;
pub const RIGHT_NAME_FIELD_X: f32 = 9.0;
pub const RIGHT_NAME_FIELD_W: f32 = 136.0;
pub const RIGHT_NAME_ACTION_X: [f32; 3] = [153.0, 181.0, 209.0];
pub const SECTION_TITLE_HEIGHT: f32 = 40.0;
pub const SECTION_CONTENT_X: f32 = 17.0;
pub const SECTION_CONTENT_W: f32 = 184.0;
pub const SECTION_TRAILING_X: f32 = 209.0;
pub const FIELD_HEIGHT: f32 = 24.0;
pub const FIELD_WIDTH_HALF: f32 = 88.0;
pub const FIELD_COL_X: [f32; 2] = [17.0, 113.0];
pub const FIELD_PREFIX_W: f32 = 23.0;
pub const SUBLABEL_HEIGHT: f32 = 16.0;
pub const SEGMENT_W: f32 = 29.0;
pub const SEGMENT_GROUP_GAP: f32 = 8.0;
pub const SWATCH_SIZE: f32 = 16.0;
pub const SWATCH_RADIUS: u8 = 2;
pub const SWATCH_ROW_X: f32 = 17.0;
pub const HEX_FIELD_W: f32 = 72.0;
pub const STYLE_ROW_H: f32 = 28.0;
pub const STYLE_FOLDER_ROW_H: f32 = 24.0;
pub const STYLE_INDENT: f32 = 12.0;
pub const OPACITY_FIELD_W: f32 = 48.0;

pub const TOOLBELT_HEIGHT: f32 = 48.0;
pub const TOOLBELT_BOTTOM_MARGIN: f32 = 12.0;
pub const TOOLBELT_PADDING: f32 = 8.0;
pub const TOOL_BUTTON: f32 = 32.0;
pub const TOOL_CHEVRON_W: f32 = 16.0;
pub const TOOL_CHEVRON_GAP: f32 = 1.0;
pub const TOOL_GROUP_GAP: f32 = 8.0;
pub const TOOL_ICON_SIZE: f32 = 20.0;
pub const MODE_SWITCH_W: f32 = 28.0;

pub const MENU_WIDTH: f32 = 200.0;
pub const MENU_ITEM_HEIGHT: f32 = 24.0;
pub const MENU_PADDING: f32 = 8.0;
pub const MENU_RADIUS: u8 = 5;
pub const TOAST_RADIUS: u8 = 13;
pub const TOAST_HEIGHT: f32 = 40.0;
pub const TOAST_BOTTOM: f32 = 72.0;
pub const TOAST_SECONDS: f64 = 4.0;

// ---------------------------------------------------------------- canvas
pub const CANVAS_OUTLINE_WIDTH: f32 = 1.0;
pub const CANVAS_HOVER_WIDTH: f32 = 1.0;
pub const CANVAS_HANDLE_SIZE: f32 = 7.0;
pub const CANVAS_HANDLE_BORDER: f32 = 1.0;
pub const CANVAS_ROTATE_ZONE: f32 = 12.0;
pub const CANVAS_HANDLE_HIT: f32 = 6.0;
pub const CANVAS_BADGE_OFFSET: f32 = 4.0;
pub const CANVAS_BADGE_PAD_X: f32 = 4.0;
pub const CANVAS_BADGE_HEIGHT: f32 = 16.0;
pub const CANVAS_BADGE_RADIUS: u8 = 2;
pub const CANVAS_LABEL_GAP: f32 = 4.0;
pub const CANVAS_LABEL_MAX_ZOOM_HIDE: f32 = 0.05;
pub const SNAP_THRESHOLD_PX: f32 = 5.0;
pub const GUIDE_WIDTH: f32 = 1.0;
pub const GUIDE_X_SIZE: f32 = 3.0;
pub const GUIDE_TEXT_PAD: f32 = 3.0;
pub const GUIDE_TEXT_HEIGHT: f32 = 14.0;
pub const PEN_POINT_SIZE: f32 = 4.0;
pub const PEN_LINE_WIDTH: f32 = 1.5;
pub const DRAG_THRESHOLD_PX: f32 = 3.0;
pub const ZOOM_MIN: f32 = 0.02;
pub const ZOOM_MAX: f32 = 256.0;
pub const FIT_PADDING: f32 = 64.0;
pub const TEXT_CARET_WIDTH: f32 = 1.0;
pub const NUDGE: f32 = 1.0;
pub const NUDGE_BIG: f32 = 10.0;

// ---------------------------------------------------------------- app defaults
pub const DEFAULT_WINDOW: (f32, f32) = (1600.0, 1088.0);
pub const MIN_WINDOW: (f32, f32) = (900.0, 600.0);
pub const APP_TITLE: &str = "desigma";
pub const DEFAULT_SHAPE_SIZE: f32 = 100.0;
pub const DEFAULT_TEXT_SIZE: f32 = 16.0;
pub const DEFAULT_PAGE_BG: u32 = 0xF5F5F5;
pub const DEFAULT_STROKE: u32 = 0x000000;
pub const DEFAULT_FRAME_FILL: u32 = 0xFFFFFF;
pub const DEFAULT_SHAPE_FILL: u32 = 0xD9D9D9;
pub const AUTO_LAYOUT_SPACING: f32 = 10.0;
pub const AUTO_LAYOUT_PADDING: f32 = 10.0;
pub const PASTE_OFFSET: f32 = 10.0;

/// Applies the Figma-like dark visuals to egui so any stock widget we do use
/// (text edits, scroll bars, popups) matches the chrome.
pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();
    let v = &mut style.visuals;
    *v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG_MENU;
    v.extreme_bg_color = BG_SECONDARY;
    v.faint_bg_color = BG_SECONDARY;
    v.override_text_color = None;
    v.window_stroke = egui::Stroke::new(1.0, BORDER_MENU);
    v.window_corner_radius = cr(RADIUS_MEDIUM);
    v.menu_corner_radius = cr(RADIUS_MEDIUM);
    v.selection.bg_fill = BG_SELECTED;
    v.selection.stroke = egui::Stroke::new(1.0, BORDER_SELECTED);
    v.text_cursor.stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_fill = BG;
    v.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, TEXT);
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, BORDER);
    for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = cr(RADIUS_MEDIUM);
        w.fg_stroke = egui::Stroke::new(1.0, TEXT);
    }
    v.widgets.inactive.bg_fill = BG_SECONDARY;
    v.widgets.inactive.weak_bg_fill = BG_SECONDARY;
    v.widgets.hovered.bg_fill = BG_HOVER;
    v.widgets.hovered.weak_bg_fill = BG_HOVER;
    v.widgets.active.bg_fill = BG_PRESSED;
    v.widgets.active.weak_bg_fill = BG_PRESSED;
    style.spacing.item_spacing = egui::vec2(0.0, 0.0);
    style.spacing.button_padding = egui::vec2(0.0, 0.0);
    style.spacing.interact_size = egui::vec2(0.0, 0.0);
    style.spacing.scroll.bar_width = 6.0;
    style.spacing.scroll.floating = true;
    style.text_styles.insert(egui::TextStyle::Body, font_body());
    style.text_styles.insert(egui::TextStyle::Button, font_body());
    style.text_styles.insert(egui::TextStyle::Small, font_small());
    style.text_styles.insert(egui::TextStyle::Monospace, font_mono());
    ctx.set_global_style(style);
}

/// Installs the bundled Inter variable font as two instances: regular and strong.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let inter = desigma_fonts::bundled()
        .iter()
        .find(|a| a.family == UI_FONT_FAMILY && !a.italic)
        .expect("bundled Inter");
    let instance = |weight: f32| {
        let tweak = egui::FontTweak {
            coords: egui::epaint::text::VariationCoords::new([("wght", weight), ("opsz", UI_FONT_OPSZ)]),
            ..Default::default()
        };
        std::sync::Arc::new(egui::FontData::from_static(inter.data).tweak(tweak))
    };
    fonts.font_data.insert("ui-regular".into(), instance(UI_FONT_WEIGHT));
    fonts.font_data.insert("ui-strong".into(), instance(UI_FONT_WEIGHT_STRONG));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "ui-regular".into());
    let mut strong_family = vec!["ui-strong".to_string()];
    strong_family.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name(FAMILY_STRONG.into()), strong_family);
    ctx.set_fonts(fonts);
}

/// Perceived luminance test used to choose label colors on the canvas.
pub fn is_light(c: Color32) -> bool {
    let l = (0.299 * c.r() as f32 + 0.587 * c.g() as f32 + 0.114 * c.b() as f32) / 255.0;
    l > LIGHT_BG_THRESHOLD
}
