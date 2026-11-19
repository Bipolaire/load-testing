//! Système de design type Delta / Zed : surfaces sobres, typo compacte, mono pour les données.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Div, ElementId, Entity, Hsla, IntoElement, ParentElement, SharedString, Styled,
    Window, div, px, rgb,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::{Input, InputState};
use gpui_component::notification::Notification;
use gpui_component::theme::{Colorize, Theme};
use gpui_component::{ActiveTheme, Icon, Sizable, Size, WindowExt};

/// Texte courant.
pub const BODY_FONT: &str = "iA Writer Quattro S";
/// Titres.
pub const DISPLAY_FONT: &str = "IBM Plex Serif";
/// Libellés d'interface et données.
pub const INTERFACE_FONT: &str = "Lilex";

/// Largeur par défaut des barres latérales (identique sur tous les onglets).
pub const SIDEBAR_WIDTH: f32 = 280.;
pub const SIDEBAR_MIN_WIDTH: f32 = 200.;
pub const SIDEBAR_MAX_WIDTH: f32 = 420.;
/// Hauteur commune des en-têtes de barre latérale et des barres d'outils, pour
/// que leurs bordures inférieures se prolongent d'un panneau à l'autre.
pub const HEADER_HEIGHT: f32 = 44.;

/// Violet de la marque (seul accent).
pub fn brand_primary() -> Hsla {
    rgb(0x5D5FEF).into()
}

#[allow(dead_code)]
pub fn brand_accent() -> Hsla {
    rgb(0xF470B4).into()
}

fn hex(value: u32) -> Hsla {
    rgb(value).into()
}

/// Applique la palette Delta complète sur le thème actif.
pub fn apply_theme(cx: &mut App) {
    let brand = brand_primary();
    let theme = Theme::global_mut(cx);
    let dark = theme.is_dark();

    theme.font_family = BODY_FONT.into();
    theme.mono_font_family = INTERFACE_FONT.into();
    theme.font_size = px(13.);
    theme.mono_font_size = px(12.);
    theme.radius = px(0.);
    theme.radius_lg = px(0.);
    theme.shadow = false;

    if dark {
        theme.background = hex(0x0E0E10);
        theme.foreground = hex(0xE6E6EA);
        theme.muted = hex(0x1A1A1E);
        theme.muted_foreground = hex(0x8B8B94);
        theme.border = hex(0x26262B);
        theme.input = hex(0x26262B);
        theme.secondary = hex(0x1A1A1E);
        theme.secondary_foreground = hex(0xE6E6EA);
        theme.secondary_hover = hex(0x222228);
        theme.secondary_active = hex(0x2A2A32);
        theme.accent = hex(0x1A1A1E);
        theme.accent_foreground = hex(0xE6E6EA);
        theme.popover = hex(0x141417);
        theme.popover_foreground = hex(0xE6E6EA);
        theme.sidebar = hex(0x141417);
        theme.sidebar_foreground = hex(0xE6E6EA);
        theme.sidebar_border = hex(0x26262B);
        theme.sidebar_accent = brand.opacity(0.14);
        theme.sidebar_accent_foreground = hex(0xE6E6EA);
        theme.title_bar = hex(0x141417);
        theme.title_bar_border = hex(0x26262B);
        theme.tab_bar = hex(0x0E0E10);
        theme.tab_bar_segmented = hex(0x1A1A1E);
        theme.tab = hex(0x0E0E10);
        theme.tab_active = hex(0x1A1A1E);
        theme.tab_active_foreground = hex(0xE6E6EA);
        theme.tab_foreground = hex(0x8B8B94);
        theme.list = hex(0x141417);
        theme.list_even = hex(0x141417);
        theme.list_hover = hex(0x1A1A1E);
        theme.list_active = brand.opacity(0.14);
        theme.list_active_border = brand.opacity(0.45);
        theme.list_head = hex(0x141417);
        theme.table = hex(0x141417);
        theme.table_even = hex(0x16161A);
        theme.table_hover = hex(0x1A1A1E);
        theme.table_active = brand.opacity(0.12);
        theme.table_active_border = brand.opacity(0.4);
        theme.table_head = hex(0x141417);
        theme.table_head_foreground = hex(0x8B8B94);
        theme.table_row_border = hex(0x26262B);
        theme.scrollbar = hex(0x0E0E10).opacity(0.0);
        theme.scrollbar_thumb = hex(0x3A3A42);
        theme.scrollbar_thumb_hover = hex(0x4A4A54);
        theme.overlay = hex(0x000000).opacity(0.55);
        theme.group_box = hex(0x1A1A1E);
        theme.group_box_foreground = hex(0xE6E6EA);
        theme.accordion = hex(0x141417);
        theme.accordion_hover = hex(0x1A1A1E);
        theme.skeleton = hex(0x1A1A1E);
        theme.tiles = hex(0x0E0E10);
        theme.switch = hex(0x2A2A32);
        theme.switch_thumb = hex(0xE6E6EA);
        theme.slider_bar = brand;
        theme.slider_thumb = hex(0xFFFFFF);
        theme.progress_bar = brand;
        theme.caret = brand;
        theme.selection = brand.opacity(0.28);
        theme.chart_1 = brand;
        theme.chart_2 = hex(0x34D399);
        theme.chart_3 = hex(0xFBBF24);
        theme.chart_4 = hex(0xF470B4);
        theme.chart_5 = hex(0x60A5FA);
    } else {
        theme.background = hex(0xFAFAFA);
        theme.foreground = hex(0x18181B);
        theme.muted = hex(0xF4F4F5);
        theme.muted_foreground = hex(0x71717A);
        theme.border = hex(0xE4E4E7);
        theme.input = hex(0xE4E4E7);
        theme.secondary = hex(0xF4F4F5);
        theme.secondary_foreground = hex(0x18181B);
        theme.secondary_hover = hex(0xE4E4E7);
        theme.secondary_active = hex(0xD4D4D8);
        theme.accent = hex(0xF4F4F5);
        theme.accent_foreground = hex(0x18181B);
        theme.popover = hex(0xFFFFFF);
        theme.popover_foreground = hex(0x18181B);
        theme.sidebar = hex(0xF4F4F5);
        theme.sidebar_foreground = hex(0x18181B);
        theme.sidebar_border = hex(0xE4E4E7);
        theme.sidebar_accent = brand.opacity(0.12);
        theme.sidebar_accent_foreground = hex(0x18181B);
        theme.title_bar = hex(0xF4F4F5);
        theme.title_bar_border = hex(0xE4E4E7);
        theme.tab_bar = hex(0xFAFAFA);
        theme.tab_bar_segmented = hex(0xE4E4E7);
        theme.tab = hex(0xFAFAFA);
        theme.tab_active = hex(0xFFFFFF);
        theme.tab_active_foreground = hex(0x18181B);
        theme.tab_foreground = hex(0x71717A);
        theme.list = hex(0xF4F4F5);
        theme.list_even = hex(0xF4F4F5);
        theme.list_hover = hex(0xE4E4E7);
        theme.list_active = brand.opacity(0.12);
        theme.list_active_border = brand.opacity(0.4);
        theme.list_head = hex(0xF4F4F5);
        theme.table = hex(0xFFFFFF);
        theme.table_even = hex(0xFAFAFA);
        theme.table_hover = hex(0xF4F4F5);
        theme.table_active = brand.opacity(0.10);
        theme.table_active_border = brand.opacity(0.35);
        theme.table_head = hex(0xF4F4F5);
        theme.table_head_foreground = hex(0x71717A);
        theme.table_row_border = hex(0xE4E4E7);
        theme.scrollbar = hex(0xFAFAFA).opacity(0.0);
        theme.scrollbar_thumb = hex(0xD4D4D8);
        theme.scrollbar_thumb_hover = hex(0xA1A1AA);
        theme.overlay = hex(0x000000).opacity(0.35);
        theme.group_box = hex(0xF4F4F5);
        theme.group_box_foreground = hex(0x18181B);
        theme.accordion = hex(0xFAFAFA);
        theme.accordion_hover = hex(0xF4F4F5);
        theme.skeleton = hex(0xE4E4E7);
        theme.tiles = hex(0xFAFAFA);
        theme.switch = hex(0xD4D4D8);
        theme.switch_thumb = hex(0xFFFFFF);
        theme.slider_bar = brand;
        theme.slider_thumb = hex(0xFFFFFF);
        theme.progress_bar = brand;
        theme.caret = brand;
        theme.selection = brand.opacity(0.22);
        theme.chart_1 = brand;
        theme.chart_2 = hex(0x059669);
        theme.chart_3 = hex(0xD97706);
        theme.chart_4 = hex(0xDB2777);
        theme.chart_5 = hex(0x2563EB);
    }

    theme.primary = brand;
    theme.primary_foreground = hex(0xFFFFFF);
    theme.primary_hover = if dark {
        brand.lighten(0.08)
    } else {
        brand.darken(0.06)
    };
    theme.primary_active = if dark {
        brand.lighten(0.14)
    } else {
        brand.darken(0.12)
    };
    theme.ring = brand;
    theme.sidebar_primary = brand;
    theme.sidebar_primary_foreground = hex(0xFFFFFF);
    theme.link = brand;
    theme.link_hover = brand.lighten(0.1);
    theme.link_active = brand.darken(0.05);
}


pub fn icon(name: &str) -> Icon {
    Icon::default().path(format!("icons/{name}.svg"))
}

pub fn elevated(cx: &App) -> Hsla {
    if cx.theme().is_dark() {
        hex(0x1A1A1E)
    } else {
        hex(0xFFFFFF)
    }
}

/// Surface avec bordure fine, sans ombre.
pub fn surface(cx: &App) -> Div {
    div()
        .rounded(cx.theme().radius)
        .bg(elevated(cx))
        .border_1()
        .border_color(cx.theme().border)
}

/// Ancien `card` → surface Delta.
pub fn card(cx: &App) -> Div {
    surface(cx).p_4()
}

/// Libellé d'interface façon delta.dev : Lilex 11px en capitales.
pub fn eyebrow(text: impl Into<SharedString>, cx: &App) -> Div {
    let text: SharedString = text.into();
    div()
        .font_family(INTERFACE_FONT)
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(cx.theme().muted_foreground)
        .child(SharedString::from(text.to_uppercase()))
}

/// Libellé de champ : Lilex 11px, casse conservée.
pub fn field_label(text: impl Into<SharedString>, cx: &App) -> Div {
    div()
        .font_family(INTERFACE_FONT)
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

pub fn section_label(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    eyebrow(text, cx)
}

pub fn section_title(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .font_family(DISPLAY_FONT)
        .text_base()
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_color(cx.theme().foreground)
        .child(text.into())
}

pub fn muted(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

pub fn mono(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .font_family(cx.theme().mono_font_family.clone())
        .text_xs()
        .text_color(cx.theme().foreground)
        .child(text.into())
}

pub fn mono_muted(text: impl Into<SharedString>, cx: &App) -> impl IntoElement {
    div()
        .font_family(cx.theme().mono_font_family.clone())
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(text.into())
}

#[allow(dead_code)]
pub fn badge(text: impl Into<SharedString>, fg: Hsla, bg: Hsla) -> impl IntoElement {
    div()
        .px_1p5()
        .py(px(2.))
        .font_family(INTERFACE_FONT)
        .text_size(px(11.))
        .line_height(px(16.))
        .text_color(fg)
        .bg(bg)
        .child(text.into())
}

/// Pastille de statut avec point coloré.
#[allow(dead_code)]
pub fn status_pill(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    chip(text, color)
}

pub fn chip(text: impl Into<SharedString>, color: Hsla) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1p5()
        .px_1p5()
        .py(px(2.))
        .bg(color.opacity(0.12))
        .border_1()
        .border_color(color.opacity(0.28))
        .child(div().size(px(6.)).rounded_full().bg(color))
        .child(
            div()
                .font_family(INTERFACE_FONT)
                .text_size(px(11.))
                .line_height(px(16.))
                .text_color(color)
                .child(text.into()),
        )
}

pub fn kpi(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    color: Option<Hsla>,
    cx: &App,
) -> impl IntoElement {
    kpi_with_hint(label, value, None::<SharedString>, color, cx)
}

pub fn kpi_with_hint(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    hint: Option<impl Into<SharedString>>,
    color: Option<Hsla>,
    cx: &App,
) -> impl IntoElement {
    stat(label, value, hint, color, cx)
}

/// KPI compact : valeur en mono, sans fond teinté.
pub fn stat(
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    hint: Option<impl Into<SharedString>>,
    color: Option<Hsla>,
    cx: &App,
) -> impl IntoElement {
    let value_color = color.unwrap_or(cx.theme().foreground);
    let mut col = div()
        .flex()
        .flex_col()
        .gap_1()
        .flex_1()
        .min_w(px(100.))
        .px_3()
        .py_2p5()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .bg(elevated(cx))
        .child(eyebrow(label, cx))
        .child(
            div()
                .font_family(cx.theme().mono_font_family.clone())
                .text_lg()
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(value_color)
                .child(value.into()),
        );
    if let Some(hint) = hint {
        col = col.child(muted(hint, cx));
    }
    col
}

pub fn empty_state_rich(
    icon_name: Option<&str>,
    title: impl Into<SharedString>,
    subtitle: Option<impl Into<SharedString>>,
    cta: Option<Button>,
    cx: &App,
) -> impl IntoElement {
    let mut col = div()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .p_10()
        .max_w(px(420.));
    if let Some(name) = icon_name {
        col = col.child(
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(48.))
                .rounded(cx.theme().radius_lg)
                .bg(cx.theme().primary.opacity(0.12))
                .border_1()
                .border_color(cx.theme().primary.opacity(0.25))
                .child(
                    icon(name)
                        .with_size(Size::Large)
                        .text_color(cx.theme().primary),
                ),
        );
    }
    col = col.child(
        div()
            .font_family(DISPLAY_FONT)
            .text_lg()
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(cx.theme().foreground)
            .child(title.into()),
    );
    if let Some(subtitle) = subtitle {
        col = col.child(
            div()
                .text_sm()
                .text_center()
                .text_color(cx.theme().muted_foreground)
                .child(subtitle.into()),
        );
    }
    if let Some(cta) = cta {
        col = col.child(div().mt_2().child(cta));
    }
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(col)
}

pub fn sidebar_header(
    title: impl Into<SharedString>,
    action: Option<Button>,
    cx: &App,
) -> AnyElement {
    sidebar_header_row(cx)
        .child(eyebrow(title, cx).flex_1())
        .when_some(action, |row, action| row.child(action))
        .into_any_element()
}

/// Conteneur d'en-tête de barre latérale, aligné sur le contenu des listes (16px).
pub fn sidebar_header_row(cx: &App) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap_2()
        .h(px(HEADER_HEIGHT))
        .px_4()
        .border_b_1()
        .border_color(cx.theme().border)
}

/// Ligne de liste compacte (28px), sélection violet léger + barre gauche.
///
/// La barre gauche est toujours présente (transparente hors sélection) pour que
/// le texte ne se décale pas à la sélection et que `pl` reste valable dans les
/// deux états.
pub fn list_row(
    id: impl Into<ElementId>,
    selected: bool,
    cx: &App,
) -> gpui::Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_row()
        .items_center()
        .h(px(28.))
        .gap_2()
        .pl(px(6.))
        .pr_2()
        .border_l_2()
        .border_color(gpui::transparent_black())
        .cursor_pointer()
        .hover(|style| style.bg(cx.theme().list_hover))
        .when(selected, |el| {
            el.bg(cx.theme().list_active)
                .border_color(cx.theme().primary)
        })
}

pub fn toolbar(cx: &App) -> Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .h(px(HEADER_HEIGHT))
        .px_4()
        .gap_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
}

pub fn icon_toolbar_button(id: &'static str, icon_name: &str, tooltip: &str) -> Button {
    Button::new(id)
        .small()
        .ghost()
        .icon(icon(icon_name))
        .tooltip(tooltip.to_string())
}

pub fn divider(cx: &App) -> AnyElement {
    div()
        .h(px(1.))
        .w_full()
        .bg(cx.theme().border)
        .into_any_element()
}

pub fn progress_bar(ratio: f64, ok: bool, cx: &App) -> AnyElement {
    let ratio = ratio.clamp(0.0, 1.0) as f32;
    let fill = if ok {
        cx.theme().success
    } else {
        cx.theme().danger
    };
    div()
        .w_full()
        .h(px(6.))
        .rounded_full()
        .bg(cx.theme().muted)
        .overflow_hidden()
        .child(div().h_full().w(gpui::relative(ratio)).rounded_full().bg(fill))
        .into_any_element()
}

pub fn field(label: &str, input: &Entity<InputState>, cx: &App) -> AnyElement {
    field_with_tooltip(label, None, input, cx)
}

pub fn field_with_tooltip(
    label: &str,
    tooltip: Option<&str>,
    input: &Entity<InputState>,
    cx: &App,
) -> AnyElement {
    labeled(label, tooltip, Input::new(input).w_full(), cx)
}

/// Ligne de libellé de champ, de hauteur fixe avec ou sans info-bulle pour que
/// les champs voisins d'une même rangée restent alignés.
pub fn field_label_row(label: &str, tooltip: Option<&str>, cx: &App) -> Div {
    let mut label_row = div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .h(px(16.))
        .child(field_label(label.to_string(), cx));
    if let Some(tip) = tooltip {
        label_row = label_row.child(
            Button::new(SharedString::from(format!("tip-{label}")))
                .ghost()
                .xsmall()
                .icon(icon("info"))
                .tooltip(tip.to_string()),
        );
    }
    label_row
}

/// Champ libellé générique, même gabarit que [`field`].
pub fn labeled(
    label: &str,
    tooltip: Option<&str>,
    control: impl IntoElement,
    cx: &App,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(160.))
        .gap_1()
        .child(field_label_row(label, tooltip, cx))
        .child(control)
        .into_any_element()
}

/// Champ libellé contenant un menu déroulant créé par [`select_button`].
///
/// Pas de `min_w` sur la colonne : avec, le déclencheur d'un `dropdown_menu`
/// reste figé à cette largeur minimale au lieu de remplir la colonne. Le minimum
/// est porté par le bouton.
pub fn select_field(label: &str, select: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .gap_1()
        .child(field_label_row(label, None, cx))
        .child(select)
        .into_any_element()
}

/// Bouton de menu déroulant ayant la hauteur d'un champ de saisie ; remplit sa
/// colonne dans [`select_field`].
pub fn select_button(id: impl Into<ElementId>, label: impl Into<SharedString>) -> Button {
    Button::new(id)
        .outline()
        .label(label)
        .dropdown_caret(true)
        .min_w(px(160.))
        .justify_start()
}

pub fn area(label: &str, input: &Entity<InputState>, cx: &App) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w(px(200.))
        .gap_1()
        .child(field_label_row(label, None, cx))
        .child(Input::new(input).w_full())
        .into_any_element()
}

pub fn row(children: Vec<AnyElement>) -> Div {
    let mut row = div().flex().flex_row().flex_wrap().gap_3();
    for child in children {
        row = row.child(child);
    }
    row
}


/// Affiche une notification toast (succès ou erreur).
pub fn notify(window: &mut Window, cx: &mut App, text: impl Into<SharedString>, error: bool) {
    let note = if error {
        Notification::error(text)
    } else {
        Notification::success(text)
    };
    window.push_notification(note, cx);
}

/// Ouvre un fichier ou un dossier avec l'application par défaut du système.
pub fn open_path(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(path).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", ""])
            .arg(path)
            .spawn();
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
    }
}
