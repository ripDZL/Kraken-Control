//! Look of the app: colour themes, fonts, the page list and small shared widgets.
//! Every colour the UI paints comes from the active [`Palette`] (`pal()`).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Frame, Margin, Painter, Rect, Response,
    RichText, Sense, Stroke, StrokeKind, TextStyle, Ui, Vec2,
};
use serde::{Deserialize, Serialize};

/// Built-in themes; the first is the default.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ThemeId {
    #[default]
    DarkGrey,
    BlackPlumb,
    White,
    /// Okabe–Ito colours that stay apart in protan-, deutan- and tritanopia,
    /// plus thicker selection outlines (test `colour_blind_roles_stay_apart`).
    ColourBlind,
}

impl ThemeId {
    pub const ALL: [ThemeId; 4] = [ThemeId::DarkGrey, ThemeId::BlackPlumb, ThemeId::White, ThemeId::ColourBlind];

    pub fn name(self) -> &'static str {
        match self {
            ThemeId::DarkGrey => "Dark Grey",
            ThemeId::BlackPlumb => "Black-Plumb",
            ThemeId::White => "White",
            ThemeId::ColourBlind => "Colour-blind",
        }
    }

    /// `--theme=` value (screenshots; not saved).
    pub fn from_arg(s: &str) -> Option<ThemeId> {
        Some(match s {
            "grey" | "dark" => ThemeId::DarkGrey,
            "plumb" => ThemeId::BlackPlumb,
            "white" | "light" => ThemeId::White,
            "colourblind" | "colorblind" | "cb" => ThemeId::ColourBlind,
            _ => return None,
        })
    }

    fn palette(self) -> &'static Palette {
        match self {
            ThemeId::DarkGrey => &DARK_GREY,
            ThemeId::BlackPlumb => &BLACK_PLUMB,
            ThemeId::White => &WHITE,
            ThemeId::ColourBlind => &COLOUR_BLIND,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum UiFont {
    Atkinson,
    JetBrainsMono,
}

/// Colour roles.
pub struct Palette {
    /// Page background behind the cards.
    pub bg: Color32,
    /// Header, page menu and status bar.
    pub chrome: Color32,
    pub card: Color32,
    /// Card outline (equal to `card` for none).
    pub card_edge: Color32,
    /// Divider lines.
    pub line: Color32,
    /// Text fields, value boxes, slider rails, tooltips.
    pub input: Color32,
    pub button: Color32,
    pub button_hover: Color32,
    pub button_text: Color32,
    /// Primary button, switches that are on, "connected", selected key outline.
    pub accent: Color32,
    pub accent_hover: Color32,
    /// Text on `accent`.
    pub on_accent: Color32,
    /// Current page in the menu, selected chips, slider fill.
    pub nav_active: Color32,
    pub nav_active_text: Color32,
    /// Titles and labels.
    pub text: Color32,
    /// Body text and values.
    pub text_body: Color32,
    /// Helper text.
    pub muted: Color32,
    pub disabled: Color32,
    /// Unsaved changes.
    pub unsaved: Color32,
    pub error: Color32,
    pub link: Color32,
    /// Keyboard background plate.
    pub plate: Color32,
    pub key_skirt: Color32,
    pub key_top: Color32,
    pub key_hover: Color32,
    pub key_selected: Color32,
    /// Layout position the keyboard has no key for.
    pub key_empty: Color32,
    pub key_legend: Color32,
    /// Pair / advanced-key badges and outlines.
    pub key_tag: Color32,
    /// Travel values on keycaps: shortest and deepest actuation (blended between).
    pub travel: [Color32; 2],
    /// Outline width of selected keys and of tagged / changed keys.
    pub select_width: f32,
    pub tag_width: f32,
    /// Dark or light surfaces (egui's base style).
    pub dark: bool,
    /// Buttons and inputs; cards add 4.
    pub radius: u8,
    pub font: UiFont,
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const DARK_GREY: Palette = Palette {
    bg: hex(0x1F2023),
    chrome: hex(0x17181A),
    card: hex(0x27292D),
    card_edge: hex(0x313338),
    line: hex(0x36383D),
    input: hex(0x313338),
    button: hex(0x36393E),
    button_hover: hex(0x42454C),
    button_text: hex(0xECEDEE),
    accent: hex(0x3CC8B4),
    accent_hover: hex(0x5ED6C5),
    on_accent: hex(0x0F1514),
    nav_active: hex(0x24504B),
    nav_active_text: hex(0x8BE5D8),
    text: hex(0xF0F1F2),
    text_body: hex(0xDCDEE1),
    muted: hex(0xABAFB6),
    disabled: hex(0x7D8188),
    unsaved: hex(0xF2B04A),
    error: hex(0xF07A6A),
    link: hex(0x5ED6C5),
    plate: hex(0x1A1B1E),
    key_skirt: hex(0x2C2E33),
    key_top: hex(0x393C42),
    key_hover: hex(0x3F4248),
    key_selected: hex(0x1F3B3A),
    key_empty: hex(0x222327),
    key_legend: hex(0xD5D8DC),
    key_tag: hex(0x3CC8B4),
    travel: TRAVEL_ON_DARK,
    select_width: 2.0,
    tag_width: 2.0,
    dark: true,
    radius: 6,
    font: UiFont::Atkinson,
};

/// The user's Black-Plumb theme file mapped onto the roles above
/// (window.bg, base.alt, base.bg, button.bg, accent.*, text.*, chat.error).
pub const BLACK_PLUMB: Palette = Palette {
    bg: hex(0x46335A),
    chrome: hex(0x000000),
    card: hex(0x000000),
    card_edge: hex(0x000000),
    line: hex(0xDF503B),
    input: hex(0x44555B),
    button: hex(0x853894),
    button_hover: hex(0x9A4AAB),
    button_text: hex(0xFFFFFF),
    accent: hex(0x7AC143),
    accent_hover: hex(0x8FD35A),
    on_accent: hex(0x000000),
    nav_active: hex(0x7AC143),
    nav_active_text: hex(0x000000),
    text: hex(0xFFFFFF),
    text_body: hex(0xF7F0E7),
    muted: hex(0xD9C7B0),
    disabled: hex(0xA48F7B),
    unsaved: hex(0xFFC30F),
    error: hex(0xFF8A75),
    link: hex(0xFFFF00),
    plate: hex(0x46335A),
    key_skirt: hex(0x586C72),
    key_top: hex(0x334146),
    key_hover: hex(0x384750),
    key_selected: hex(0x24342A),
    key_empty: hex(0x2E2240),
    key_legend: hex(0xF7F0E7),
    key_tag: hex(0x8BD452),
    travel: TRAVEL_ON_DARK,
    select_width: 2.0,
    tag_width: 2.0,
    dark: true,
    radius: 6,
    font: UiFont::JetBrainsMono,
};

/// Light grey page, white cards and keycaps, dark teal accent.
pub const WHITE: Palette = Palette {
    bg: hex(0xEEF0F3),
    chrome: hex(0xFFFFFF),
    card: hex(0xFFFFFF),
    card_edge: hex(0xDADDE2),
    line: hex(0xE1E4E8),
    input: hex(0xE6E8EC),
    button: hex(0xE4E7EB),
    button_hover: hex(0xD7DBE0),
    button_text: hex(0x16181B),
    accent: hex(0x0B7D70),
    accent_hover: hex(0x096B60),
    on_accent: hex(0xFFFFFF),
    nav_active: hex(0xD4EEEA),
    nav_active_text: hex(0x0A5A51),
    text: hex(0x14161A),
    text_body: hex(0x272A30),
    muted: hex(0x585E68),
    disabled: hex(0x7E848D),
    unsaved: hex(0x9A5400),
    error: hex(0xC0321F),
    link: hex(0x0A6E63),
    plate: hex(0xCDD2D9),
    key_skirt: hex(0xB9BFC8),
    key_top: hex(0xFBFCFD),
    key_hover: hex(0xEEF1F4),
    key_selected: hex(0xD7F0EC),
    key_empty: hex(0xE3E6EA),
    key_legend: hex(0x1E2126),
    key_tag: hex(0x0A6E63),
    travel: [hex(0x1F5FAE), hex(0xA34A00)],
    select_width: 2.0,
    tag_width: 2.0,
    dark: false,
    radius: 6,
    font: UiFont::Atkinson,
};

/// Dark theme for colour-blind users. Roles use the Okabe–Ito set (sky blue,
/// yellow, reddish purple, orange), chosen so every pair stays apart in
/// simulated protanopia, deuteranopia and tritanopia; selected keys get a
/// thick outline and tagged keys a thin one, so nothing depends on colour
/// alone (monochromacy).
pub const COLOUR_BLIND: Palette = Palette {
    bg: hex(0x1C1D20),
    chrome: hex(0x141517),
    card: hex(0x25272B),
    card_edge: hex(0x34363C),
    line: hex(0x3A3D43),
    input: hex(0x313338),
    button: hex(0x383B41),
    button_hover: hex(0x45484F),
    button_text: hex(0xF2F3F4),
    accent: hex(0x56B4E9),
    accent_hover: hex(0x7FC7F0),
    on_accent: hex(0x0B1620),
    nav_active: hex(0x2F8FC6),
    nav_active_text: hex(0x061019),
    text: hex(0xF4F5F6),
    text_body: hex(0xE0E2E5),
    muted: hex(0xB2B6BD),
    disabled: hex(0x80848B),
    unsaved: hex(0xF0E442),
    error: hex(0xCC79A7),
    link: hex(0x8FD0F5),
    plate: hex(0x17181B),
    key_skirt: hex(0x2B2D32),
    key_top: hex(0x3A3D43),
    key_hover: hex(0x44474E),
    key_selected: hex(0x0D2335),
    key_empty: hex(0x202125),
    key_legend: hex(0xE0E2E5),
    key_tag: hex(0xF5A623),
    travel: [hex(0x8CC8F5), hex(0xF5A623)],
    select_width: 3.5,
    tag_width: 1.5,
    dark: true,
    radius: 6,
    font: UiFont::Atkinson,
};

/// Short = light blue, deep = orange (blue / orange stays apart for colour-blind users too).
const TRAVEL_ON_DARK: [Color32; 2] = [hex(0x8CC3FF), hex(0xFFA05A)];

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn current() -> ThemeId {
    match CURRENT.load(Ordering::Relaxed) {
        1 => ThemeId::BlackPlumb,
        2 => ThemeId::White,
        3 => ThemeId::ColourBlind,
        _ => ThemeId::DarkGrey,
    }
}

/// Active palette.
pub fn pal() -> &'static Palette {
    current().palette()
}

/// Switches fonts, sizes and colours (cheap enough to call on a theme change, not every frame).
pub fn apply(ctx: &egui::Context, id: ThemeId) {
    CURRENT.store(id as u8, Ordering::Relaxed);
    let p = id.palette();
    ctx.set_fonts(fonts(p));
    // One style slot for every theme (light ones included), so a light or dark
    // Windows theme never swaps egui's style under us.
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_style_of(egui::Theme::Dark, style(p));
}

static ATKINSON: &[u8] = include_bytes!("../res/fonts/AtkinsonHyperlegibleNext-Regular.ttf");
static ATKINSON_BOLD: &[u8] = include_bytes!("../res/fonts/AtkinsonHyperlegibleNext-SemiBold.ttf");
static JB_MONO: &[u8] = include_bytes!("../res/fonts/JetBrainsMono-Regular.ttf");
static JB_MONO_BOLD: &[u8] = include_bytes!("../res/fonts/JetBrainsMono-Bold.ttf");
/// ← ↑ → ↓ from Hack and ★ from emoji-icon-font (subsets; egui's own fonts are not built in).
static ARROWS: &[u8] = include_bytes!("../res/fonts/KrakenArrows.ttf");
static STAR: &[u8] = include_bytes!("../res/fonts/KrakenStar.ttf");

/// Family for headings and labels.
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}

fn fonts(p: &Palette) -> FontDefinitions {
    let mut f = FontDefinitions::default();
    let (reg, bold_data): (&'static [u8], &'static [u8]) = match p.font {
        UiFont::Atkinson => (ATKINSON, ATKINSON_BOLD),
        UiFont::JetBrainsMono => (JB_MONO, JB_MONO_BOLD),
    };
    f.font_data.insert("kc-regular".into(), Arc::new(FontData::from_static(reg)));
    f.font_data.insert("kc-bold".into(), Arc::new(FontData::from_static(bold_data)));
    f.font_data.insert("kc-mono".into(), Arc::new(FontData::from_static(JB_MONO)));
    f.font_data.insert("kc-arrows".into(), Arc::new(FontData::from_static(ARROWS)));
    f.font_data.insert("kc-star".into(), Arc::new(FontData::from_static(STAR)));
    // Symbols the Latin subsets lack come from the two small fallback fonts.
    let with_symbols = |main: &str| vec![main.to_string(), "kc-arrows".into(), "kc-star".into()];
    f.families.insert(FontFamily::Proportional, with_symbols("kc-regular"));
    f.families.insert(bold(), with_symbols("kc-bold"));
    f.families.insert(FontFamily::Monospace, with_symbols("kc-mono"));
    f
}

fn style(p: &Palette) -> egui::Style {
    let mut s = egui::Style::default();
    let (body, small, head) = match p.font {
        UiFont::Atkinson => (14.5, 12.5, 21.0),
        UiFont::JetBrainsMono => (13.5, 12.0, 20.0),
    };
    s.text_styles = [
        (TextStyle::Small, FontId::proportional(small)),
        (TextStyle::Body, FontId::proportional(body)),
        (TextStyle::Button, FontId::proportional(body)),
        (TextStyle::Heading, FontId::new(head, bold())),
        (TextStyle::Monospace, FontId::monospace(small + 0.5)),
        (TextStyle::Name("section".into()), FontId::new(body + 2.0, bold())),
        (TextStyle::Name("strong".into()), FontId::new(body, bold())),
    ]
    .into();
    s.spacing.item_spacing = Vec2::new(8.0, 8.0);
    s.spacing.button_padding = Vec2::new(12.0, 5.0);
    s.spacing.interact_size = Vec2::new(40.0, 28.0);
    s.spacing.slider_width = 220.0;
    s.spacing.combo_width = 160.0;
    s.visuals = visuals(p);
    s
}

fn visuals(p: &Palette) -> egui::Visuals {
    let mut v = if p.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    let r = CornerRadius::same(p.radius);
    v.panel_fill = p.bg;
    v.window_fill = p.card;
    v.window_stroke = Stroke::new(1.0_f32, p.input);
    v.window_corner_radius = CornerRadius::same(p.radius + 2);
    v.menu_corner_radius = r;
    v.extreme_bg_color = p.input;
    v.faint_bg_color = p.card;
    v.code_bg_color = p.input;
    v.hyperlink_color = p.link;
    v.warn_fg_color = p.unsaved;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.nav_active;
    v.selection.stroke = Stroke::new(1.0_f32, p.nav_active_text);
    v.slider_trailing_fill = true;
    v.indent_has_left_vline = false;
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = p.card;
    w.noninteractive.weak_bg_fill = p.card;
    w.noninteractive.bg_stroke = Stroke::new(1.0_f32, p.line);
    w.noninteractive.fg_stroke = Stroke::new(1.0_f32, p.text_body);
    w.noninteractive.corner_radius = r;
    w.inactive.bg_fill = p.input;
    w.inactive.weak_bg_fill = p.button;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0_f32, p.button_text);
    w.inactive.corner_radius = r;
    w.inactive.expansion = 0.0;
    w.hovered.bg_fill = p.button_hover;
    w.hovered.weak_bg_fill = p.button_hover;
    w.hovered.bg_stroke = Stroke::new(1.0_f32, p.accent);
    w.hovered.fg_stroke = Stroke::new(1.5_f32, p.text);
    w.hovered.corner_radius = r;
    w.hovered.expansion = 0.0;
    // egui uses the "active" text colour for `ui.strong()` too, so it must be
    // readable on cards; pressed buttons darken instead of turning accent.
    w.active.bg_fill = p.accent;
    w.active.weak_bg_fill = p.input;
    w.active.bg_stroke = Stroke::new(1.0_f32, p.accent);
    w.active.fg_stroke = Stroke::new(1.5_f32, p.text);
    w.active.corner_radius = r;
    w.open.bg_fill = p.input;
    w.open.weak_bg_fill = p.button_hover;
    w.open.bg_stroke = Stroke::new(1.0_f32, p.accent);
    w.open.fg_stroke = Stroke::new(1.0_f32, p.text);
    w.open.corner_radius = r;
    v
}

/// Pages in the left menu (same list for both keyboard families).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Performance,
    Keymap,
    Advanced,
    Lighting,
    Profiles,
    Settings,
}

impl Page {
    pub const ALL: [Page; 6] = [Page::Performance, Page::Keymap, Page::Advanced, Page::Lighting, Page::Profiles, Page::Settings];

    pub fn title(self) -> &'static str {
        match self {
            Page::Performance => "Performance",
            Page::Keymap => "Keymap",
            Page::Advanced => "Advanced keys",
            Page::Lighting => "Lighting",
            Page::Profiles => "Profiles",
            Page::Settings => "Settings",
        }
    }

    /// `--tab=` / `--g2tab=` value (old names kept for scripts).
    pub fn from_arg(s: &str) -> Option<Page> {
        Some(match s {
            "performance" => Page::Performance,
            "remap" | "keymap" => Page::Keymap,
            "pairs" | "rdt" | "advanced" => Page::Advanced,
            "lighting" => Page::Lighting,
            "apps" | "profiles" => Page::Profiles,
            "settings" => Page::Settings,
            _ => return None,
        })
    }
}

/// Header, menu and status bar background.
pub fn chrome_frame(margin: Margin) -> Frame {
    Frame::new().fill(pal().chrome).inner_margin(margin)
}

/// Full-width content card.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal();
    Frame::new()
        .fill(p.card)
        .stroke(Stroke::new(1.0_f32, p.card_edge))
        .corner_radius(CornerRadius::same(p.radius + 4))
        .inner_margin(Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Bold label text.
pub fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text).text_style(TextStyle::Name("strong".into())).color(pal().text)
}

pub fn page_header(ui: &mut Ui, title: &str, blurb: &str) {
    page_header_with(ui, title, blurb, |_| {});
}

/// Page title with optional controls on the same row (e.g. lighting presets),
/// then a one-line description. Always the same height, so the keyboard below
/// starts at the same place on every page.
pub fn page_header_with(ui: &mut Ui, title: &str, blurb: &str, extra: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.set_height(34.0);
        ui.label(RichText::new(title).text_style(TextStyle::Heading).color(pal().text));
        ui.add_space(16.0);
        extra(ui);
    });
    // Too long for the window: cut with "…", full text on hover.
    ui.add(egui::Label::new(RichText::new(blurb).color(pal().muted)).truncate());
    ui.add_space(6.0);
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.label(RichText::new(title).text_style(TextStyle::Name("section".into())).color(pal().text));
}

/// Section title with controls on the right of the same row (added right to
/// left: the first control ends up rightmost).
pub fn section_with(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        section(ui, title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

/// Width for sliders in `setting` rows: what is left after the label column
/// and the value box.
pub fn setting_slider_width(ui: &Ui) -> f32 {
    let w = ui.available_width();
    (w - (w * 0.42).clamp(170.0, 340.0) - 110.0).clamp(110.0, 360.0)
}

/// Placeholder text for empty text fields (egui's own hint colour is too faint on some themes).
pub fn hint(text: &str) -> RichText {
    RichText::new(text).color(pal().muted)
}

/// Muted helper line.
pub fn help(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).small().color(pal().muted));
}

/// Warning line in the unsaved/attention colour.
pub fn note(ui: &mut Ui, text: impl Into<String>) {
    ui.label(RichText::new(text).color(pal().unsaved));
}

/// Label and helper text on the left, control on the right.
pub fn setting<R>(ui: &mut Ui, label: &str, help_text: &str, control: impl FnOnce(&mut Ui) -> R) -> R {
    let label_w = (ui.available_width() * 0.42).clamp(170.0, 340.0);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(Vec2::new(label_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
            ui.set_width(label_w);
            ui.label(strong(label));
            if !help_text.is_empty() {
                help(ui, help_text);
            }
        });
        control(ui)
    })
    .inner
}

/// On/off switch.
pub fn switch(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, mut r) = ui.allocate_exact_size(Vec2::new(40.0, 22.0), Sense::click());
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    let enabled = ui.is_enabled();
    let value = *on;
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, enabled, value, ""));
    if ui.is_rect_visible(rect) {
        let p = pal();
        let t = ui.ctx().animate_bool_responsive(r.id, *on);
        let rr = rect.height() / 2.0;
        ui.painter().rect_filled(rect, rr, if *on { p.accent } else { p.input });
        if r.hovered() {
            ui.painter().rect_stroke(rect, rr, Stroke::new(1.0_f32, p.accent_hover), StrokeKind::Outside);
        }
        let x = egui::lerp((rect.left() + rr)..=(rect.right() - rr), t);
        ui.painter().circle_filled(egui::pos2(x, rect.center().y), rr - 4.0, if *on { p.on_accent } else { p.text_body });
    }
    r
}

/// Switch with its label and helper text (whole-keyboard options).
pub fn switch_row(ui: &mut Ui, on: &mut bool, label: &str, help_text: &str) -> Response {
    setting(ui, label, help_text, |ui| switch(ui, on))
}

/// The one main action on screen: accent colour while it can be used, a
/// plain (greyed) button otherwise.
pub fn primary_button(ui: &mut Ui, enabled: bool, text: &str) -> Response {
    let p = pal();
    let label = RichText::new(text).text_style(TextStyle::Name("strong".into()));
    let button = if enabled { egui::Button::new(label.color(p.on_accent)).fill(p.accent) } else { egui::Button::new(label) };
    ui.add_enabled(enabled, button)
}

pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.0, color);
}

/// One piece of a header row, added in reading order or reversed.
type Part<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// Keyboard name and state in the header. `switch`: shows a swap icon and reacts to clicks.
pub fn pill(ui: &mut Ui, color: Color32, name: &str, state: &str, switch: bool) -> Response {
    let p = pal();
    let r = Frame::new()
        .stroke(Stroke::new(1.0_f32, p.input))
        .corner_radius(CornerRadius::same(200))
        .inner_margin(Margin::symmetric(10, 4))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                // Inside a right-to-left header group the row runs right to left too.
                let mut parts: Vec<Part<'_>> = vec![
                    Box::new(|ui: &mut Ui| dot(ui, color)),
                    Box::new(|ui: &mut Ui| {
                        ui.label(strong(name));
                    }),
                ];
                if !state.is_empty() {
                    parts.push(Box::new(|ui: &mut Ui| {
                        ui.label(RichText::new(state).color(p.muted));
                    }));
                }
                if switch {
                    parts.push(Box::new(swap_icon));
                }
                if ui.layout().prefer_right_to_left() {
                    parts.reverse();
                }
                for add in parts {
                    add(ui);
                }
            });
        })
        .response;
    if switch {
        r.interact(Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        r
    }
}

/// Two opposite arrows (switch between keyboards).
fn swap_icon(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    let s = Stroke::new(1.5_f32, pal().muted);
    let c = rect.center();
    let painter = ui.painter();
    painter.line_segment([c + Vec2::new(-5.0, -2.5), c + Vec2::new(5.0, -2.5)], s);
    painter.line_segment([c + Vec2::new(5.0, -2.5), c + Vec2::new(2.5, -5.0)], s);
    painter.line_segment([c + Vec2::new(5.0, 2.5), c + Vec2::new(-5.0, 2.5)], s);
    painter.line_segment([c + Vec2::new(-5.0, 2.5), c + Vec2::new(-2.5, 5.0)], s);
}

/// Left page menu. `dirty` marks pages with unsaved edits, `enabled` greys out pages.
pub fn nav(ui: &mut Ui, page: &mut Page, dirty: impl Fn(Page) -> bool, enabled: impl Fn(Page) -> bool) -> bool {
    let p = pal();
    let size = TextStyle::Body.resolve(ui.style()).size;
    let mut changed = false;
    for pg in Page::ALL {
        let on = enabled(pg);
        let (rect, r) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 40.0), if on { Sense::click() } else { Sense::hover() });
        let current = *page == pg;
        let painter = ui.painter();
        if current {
            painter.rect_filled(rect, p.radius, p.nav_active);
        } else if on && r.hovered() {
            painter.rect_filled(rect, p.radius, p.button_hover);
        }
        let (color, family) = match (current, on) {
            (true, _) => (p.nav_active_text, bold()),
            (false, true) => (p.text, FontFamily::Proportional),
            (false, false) => (p.disabled, FontFamily::Proportional),
        };
        painter.text(rect.left_center() + Vec2::new(14.0, 0.0), Align2::LEFT_CENTER, pg.title(), FontId::new(size, family), color);
        if dirty(pg) {
            let c = rect.right_center() - Vec2::new(16.0, 0.0);
            if current {
                painter.circle_filled(c, 5.5, p.on_accent);
            }
            painter.circle_filled(c, 4.0, p.unsaved);
        }
        if r.clicked() && !current {
            *page = pg;
            changed = true;
        }
        ui.add_space(2.0);
    }
    changed
}

/// Last status message, bottom bar.
pub fn status_line(ui: &mut Ui, text: &str, is_error: bool) {
    let p = pal();
    let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
    let c = r.center();
    if is_error {
        ui.painter().circle_stroke(c, 5.5, Stroke::new(1.6_f32, p.error));
        ui.painter().line_segment([c + Vec2::new(-2.5, -2.5), c + Vec2::new(2.5, 2.5)], Stroke::new(1.6_f32, p.error));
        ui.painter().line_segment([c + Vec2::new(-2.5, 2.5), c + Vec2::new(2.5, -2.5)], Stroke::new(1.6_f32, p.error));
    } else {
        let s = Stroke::new(1.8_f32, p.accent);
        ui.painter().line_segment([c + Vec2::new(-4.5, 0.5), c + Vec2::new(-1.5, 3.5)], s);
        ui.painter().line_segment([c + Vec2::new(-1.5, 3.5), c + Vec2::new(4.5, -3.5)], s);
    }
    // One line; long messages end in "…" with the full text on hover.
    ui.add(egui::Label::new(RichText::new(text).small().color(if is_error { p.error } else { p.text_body })).truncate());
}

/// How one keycap looks.
#[derive(Clone, Copy, Default)]
pub struct Cap {
    /// Custom top colour (per-key lighting).
    pub fill: Option<Color32>,
    pub selected: bool,
    pub hovered: bool,
    /// Outline for pair / changed keys.
    pub edge: Option<Color32>,
    /// No key at this layout position.
    pub empty: bool,
}

/// Two-tone keycap (skirt plus inset top face). Returns the top face.
pub fn keycap(painter: &Painter, rect: Rect, cap: Cap) -> Rect {
    let p = pal();
    let r = (rect.height() * 0.14).clamp(3.0, 8.0);
    if cap.empty {
        painter.rect_filled(rect, r, p.key_empty);
        return rect;
    }
    painter.rect_filled(rect, r, p.key_skirt);
    let h = rect.height();
    let top = Rect::from_min_max(rect.min + Vec2::new(h * 0.07, h * 0.05), rect.max - Vec2::new(h * 0.07, h * 0.14));
    let face = cap.fill.unwrap_or(if cap.selected {
        p.key_selected
    } else if cap.hovered {
        p.key_hover
    } else {
        p.key_top
    });
    painter.rect_filled(top, r * 0.75, face);
    let edge = if cap.selected {
        Some((p.accent, p.select_width))
    } else if let Some(c) = cap.edge {
        Some((c, p.tag_width))
    } else if cap.hovered && cap.fill.is_some() {
        Some((p.text, 2.0))
    } else {
        None
    };
    if let Some((c, w)) = edge {
        painter.rect_stroke(rect, r, Stroke::new(w, c), StrokeKind::Inside);
    }
    top
}

/// Key size for the keyboard drawings. Depends on the width only, so the
/// keyboard is the same size on every page and for both keyboard families.
/// Whole pixels, so key text uses few distinct font sizes (each size adds glyphs to the font atlas).
pub fn key_unit(ui: &Ui) -> f32 {
    (ui.available_width() / 16.5).floor().clamp(30.0, 64.0)
}

/// Font size on keycaps, rounded to whole pixels for the same reason.
pub fn key_font(unit: f32, factor: f32, min: f32, max: f32) -> f32 {
    (unit * factor).round().clamp(min, max)
}

/// Draws one line of key text only where it fits inside `space` (never on top
/// of other text). Returns whether it was drawn.
pub fn key_text_if_fits(painter: &Painter, space: Rect, text: String, font: FontId, color: Color32) -> bool {
    let galley = painter.layout_no_wrap(text, font, color);
    let r = Align2::LEFT_CENTER.anchor_size(space.left_center(), galley.size());
    if space.width() >= galley.size().x && space.height() >= galley.size().y {
        painter.galley(r.min, galley, color);
        true
    } else {
        false
    }
}

/// Travel in mm for keycaps and tooltips: "0.5", "1.25"; 0 = keyboard default.
pub fn mm(v: f32) -> String {
    if v <= 0.0 {
        return "default".into();
    }
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Plate behind the keys.
pub fn plate(painter: &Painter, rect: Rect) {
    painter.rect_filled(rect.expand(8.0), 10.0, pal().plate);
}

/// Legend text on a key with a custom top colour.
pub fn ink_on(c: Color32) -> Color32 {
    let (dark, light) = (Color32::BLACK, Color32::WHITE);
    if contrast(dark, c) >= contrast(light, c) {
        dark
    } else {
        light
    }
}

/// Colour for travel values on keys: short = blue, deep = orange (readable on keycaps).
pub fn travel_color(mm: f32) -> Color32 {
    travel_color_in(pal(), mm)
}

fn travel_color_in(p: &Palette, mm: f32) -> Color32 {
    let t = ((mm - 0.2) / 3.1).clamp(0.0, 1.0);
    let [a, b] = p.travel;
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

/// WCAG relative luminance.
fn luminance(c: Color32) -> f64 {
    let lin = |v: u8| {
        let v = v as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
}

/// WCAG contrast ratio (1..21).
pub fn contrast(a: Color32, b: Color32) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// What the user did in [`presets_row`].
pub enum PresetAction {
    Apply(usize),
    Save(String),
    Replace(usize),
    Delete(usize),
}

/// Saved lighting looks as one row of buttons plus "Save current…".
/// `active` marks the preset that matches the lighting shown now.
pub fn presets_row(ui: &mut Ui, names: &[String], active: Option<usize>, new_name: &mut String) -> Option<PresetAction> {
    let mut act = None;
    egui::ScrollArea::horizontal().id_salt("presets_row").show(ui, |ui| ui.horizontal(|ui| {
        ui.label(strong("Presets"));
        ui.add_space(4.0);
        for (i, name) in names.iter().enumerate() {
            let r = ui
                .add(egui::Button::new(name.as_str()).selected(active == Some(i)))
                .on_hover_text("Click to apply. Right-click to replace or delete.");
            if r.clicked() {
                act = Some(PresetAction::Apply(i));
            }
            r.context_menu(|ui| {
                if ui.button("Replace with current lighting").clicked() {
                    act = Some(PresetAction::Replace(i));
                    ui.close_menu();
                }
                if ui.button("Delete").clicked() {
                    act = Some(PresetAction::Delete(i));
                    ui.close_menu();
                }
            });
        }
        if !names.is_empty() {
            ui.separator();
        }
        ui.menu_button("Save current…", |ui| {
            ui.set_min_width(230.0);
            ui.label(strong("Save current lighting"));
            let edit = ui.add(egui::TextEdit::singleline(new_name).hint_text(hint("Name, e.g. Gaming")).desired_width(210.0));
            // Ready to type as soon as the menu opens.
            if new_name.is_empty() && !edit.has_focus() {
                edit.request_focus();
            }
            let name = new_name.trim().to_string();
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let exists = names.contains(&name);
            if exists {
                help(ui, "A preset with this name exists and will be replaced.");
            }
            let save = primary_button(ui, !name.is_empty(), if exists { "Replace preset" } else { "Save preset" }).clicked();
            if (save || enter) && !name.is_empty() {
                act = Some(PresetAction::Save(name));
                new_name.clear();
                ui.close_menu();
            }
        });
        if names.is_empty() {
            help(ui, "Save the lighting you have now to switch back to it with one click.");
        }
    }));
    act
}

/// Shown instead of the editors while no keyboard is connected.
pub fn no_keyboard(ui: &mut Ui) {
    let p = pal();
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.12).max(16.0));
        let (rect, _) = ui.allocate_exact_size(Vec2::new(300.0, 112.0), Sense::hover());
        let painter = ui.painter();
        let s = Stroke::new(1.5_f32, p.input);
        painter.rect_stroke(rect, 12.0, s, StrokeKind::Inside);
        let row = |y: f32, h: f32, xs: &[(f32, f32)]| {
            for &(x, w) in xs {
                let r = Rect::from_min_size(rect.min + Vec2::new(x, y), Vec2::new(w, h));
                painter.rect_stroke(r, 3.0, s, StrokeKind::Inside);
            }
        };
        row(16.0, 16.0, &[(16.0, 250.0)]);
        row(38.0, 16.0, &[(16.0, 268.0)]);
        row(60.0, 16.0, &[(16.0, 268.0)]);
        row(82.0, 16.0, &[(16.0, 64.0), (86.0, 124.0), (216.0, 68.0)]);
        painter.circle_stroke(rect.min + Vec2::new(280.0, 24.0), 7.0, Stroke::new(1.5_f32, p.accent));
        ui.add_space(12.0);
        ui.label(RichText::new("Plug in your keyboard").text_style(TextStyle::Heading).color(p.text));
        ui.label(RichText::new("Connect your DrunkDeer keyboard with its USB cable. Kraken Control finds it on its own, no restart needed.").color(p.muted));
        ui.add_space(4.0);
        ui.label(RichText::new("Works with A75 Master, A75 Ultra (gen 1) and A75 Ultra (2025).").small().color(p.muted));
        ui.add_space(12.0);
        ui.label(strong("Still not showing up?"));
        ui.label(RichText::new("Try another USB port or cable. If it still fails, turn on the detailed log in Settings and send the log with your report.").small().color(p.muted));
    });
}

/// Deepest actuation point on the gen-1 boards; a fully shaded key face = this depth.
const DEPTH_FULL_MM: f32 = 3.3;

/// Live key depth on a key face: shading from the top shows how far the key is
/// pressed (teal once past its actuation point), with an amber line at the
/// actuation point while the key is moving.
pub fn depth_overlay(painter: &Painter, face: Rect, depth_mm: f32, ap_mm: f32) {
    depth_overlay_to(painter, face, depth_mm, ap_mm, DEPTH_FULL_MM);
}

/// [`depth_overlay`] with the depth that fills the whole key face (switch travel).
pub fn depth_overlay_to(painter: &Painter, face: Rect, depth_mm: f32, ap_mm: f32, full_mm: f32) {
    if depth_mm <= 0.0 {
        return;
    }
    let p = pal();
    let y = |mm: f32| face.min.y + face.height() * (mm / full_mm.max(0.1)).clamp(0.0, 1.0);
    let on = depth_mm >= ap_mm;
    let fill = if on { p.accent.gamma_multiply(0.45) } else { p.muted.gamma_multiply(0.28) };
    let shade = Rect::from_min_max(face.min, egui::pos2(face.max.x, y(depth_mm)));
    painter.rect_filled(shade, CornerRadius::same(p.radius.saturating_sub(1)), fill);
    let ap = y(ap_mm);
    painter.line_segment([egui::pos2(face.min.x + 2.0, ap), egui::pos2(face.max.x - 2.0, ap)], Stroke::new(1.5_f32, p.unsaved));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// egui greys out disabled widgets by mixing every colour 50/50 with this.
    fn faded(c: Color32, to: Color32) -> Color32 {
        egui::ecolor::tint_color_towards(c, to)
    }

    /// Every text colour on every surface it is drawn on: 4.5:1 (WCAG AA),
    /// 3:1 for disabled controls and graphics.
    #[test]
    fn all_text_is_readable() {
        for id in ThemeId::ALL {
            let p = id.palette();
            let mut bad = Vec::new();
            let mut check = |what: &str, fg: Color32, bg: Color32, min: f64| {
                let r = contrast(fg, bg);
                if r < min {
                    bad.push(format!("{what}: {r:.2} < {min}"));
                }
            };
            for (bg_name, bg) in [("bg", p.bg), ("card", p.card), ("chrome", p.chrome), ("input", p.input)] {
                check(&format!("text/{bg_name}"), p.text, bg, 4.5);
                check(&format!("text_body/{bg_name}"), p.text_body, bg, 4.5);
                check(&format!("muted/{bg_name}"), p.muted, bg, 4.5);
            }
            for (bg_name, bg) in [("bg", p.bg), ("card", p.card), ("chrome", p.chrome)] {
                check(&format!("unsaved/{bg_name}"), p.unsaved, bg, 4.5);
                check(&format!("error/{bg_name}"), p.error, bg, 4.5);
                check(&format!("disabled/{bg_name}"), p.disabled, bg, 3.0);
                check(&format!("accent icon/{bg_name}"), p.accent, bg, 3.0);
            }
            check("link/card", p.link, p.card, 4.5);
            check("text/button_hover", p.text, p.button_hover, 4.5);
            check("button_text/button", p.button_text, p.button, 4.5);
            check("button_text/button_hover", p.button_text, p.button_hover, 4.5);
            check("text/pressed", p.text, p.input, 4.5);
            check("on_accent/accent", p.on_accent, p.accent, 4.5);
            check("on_accent/accent_hover", p.on_accent, p.accent_hover, 4.5);
            check("nav_active_text/nav_active", p.nav_active_text, p.nav_active, 4.5);
            check("key_tag/card", p.key_tag, p.card, 4.5);
            for (face_name, face) in [("key_top", p.key_top), ("key_hover", p.key_hover), ("key_selected", p.key_selected)] {
                check(&format!("key_legend/{face_name}"), p.key_legend, face, 4.5);
                check(&format!("text/{face_name}"), p.text, face, 4.5);
                check(&format!("muted/{face_name}"), p.muted, face, 4.5);
                check(&format!("key_tag/{face_name}"), p.key_tag, face, 4.5);
                for mm in [0.2, 1.0, 2.0, 3.3] {
                    check(&format!("travel {mm}/{face_name}"), travel_color_in(p, mm), face, 4.5);
                }
            }
            check("disabled button", faded(p.button_text, p.card), faded(p.button, p.card), 3.0);
            check("disabled label", faded(p.text_body, p.card), p.card, 3.0);
            assert!(bad.is_empty(), "{}: {bad:#?}", id.name());
        }
    }

    /// Colour-blind theme: the colours that mark states (selected, unsaved,
    /// error, advanced key) stay apart in simulated full protanopia,
    /// deuteranopia and tritanopia (Machado et al. 2009, severity 1.0), measured
    /// as OKLab distance. Monochromacy is covered by outlines, icons and text.
    #[test]
    fn colour_blind_roles_stay_apart() {
        const SIM: [(&str, [[f64; 3]; 3]); 3] = [
            ("protanopia", [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]]),
            ("deuteranopia", [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]]),
            ("tritanopia", [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]]),
        ];
        let lin = |v: u8| {
            let v = v as f64 / 255.0;
            if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let oklab = |rgb: [f64; 3]| {
            let [r, g, b] = rgb.map(|v| v.clamp(0.0, 1.0));
            let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
            let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
            let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
            [
                0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
                1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
                0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
            ]
        };
        let p = ThemeId::ColourBlind.palette();
        let roles = [("accent", p.accent), ("unsaved", p.unsaved), ("error", p.error), ("key_tag", p.key_tag)];
        let mut worst = (f64::MAX, String::new());
        for (sim, m) in SIM {
            let seen = |c: Color32| {
                let v = [lin(c.r()), lin(c.g()), lin(c.b())];
                oklab([0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2]))
            };
            for (i, (a, ca)) in roles.iter().enumerate() {
                for (b, cb) in &roles[i + 1..] {
                    let (x, y) = (seen(*ca), seen(*cb));
                    let d = ((x[0] - y[0]).powi(2) + (x[1] - y[1]).powi(2) + (x[2] - y[2]).powi(2)).sqrt();
                    if d < worst.0 {
                        worst = (d, format!("{a} / {b} in {sim}"));
                    }
                }
            }
        }
        // 0.09 OKLab is a clearly visible difference (just-noticeable is about 0.02).
        assert!(worst.0 >= 0.09, "{}: {:.3}", worst.1, worst.0);
        // Selection never relies on colour alone.
        assert!(p.select_width >= p.tag_width + 1.5);
    }

    /// Text on any per-key lighting colour stays readable.
    #[test]
    fn key_ink_is_readable_on_any_colour() {
        let mut worst = f64::MAX;
        for r in (0..=255).step_by(15) {
            for g in (0..=255).step_by(15) {
                for b in (0..=255).step_by(15) {
                    let c = Color32::from_rgb(r as u8, g as u8, b as u8);
                    worst = worst.min(contrast(ink_on(c), c));
                }
            }
        }
        assert!(worst >= 4.5, "worst {worst:.2}");
    }
}
