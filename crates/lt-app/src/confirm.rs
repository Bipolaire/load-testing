//! Dialogues de confirmation via `gpui-component::Dialog`.

use std::rc::Rc;

use gpui::{div, App, ClickEvent, ParentElement, SharedString, Styled, Window};
use gpui_component::button::ButtonVariant;
use gpui_component::dialog::DialogButtonProps;
use gpui_component::{ActiveTheme, WindowExt};
use rust_i18n::t;

/// Ouvre un dialogue de confirmation générique.
pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    message: impl Into<SharedString>,
    ok_text: impl Into<SharedString>,
    danger: bool,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) {
    let title = title.into();
    let message = message.into();
    let ok_text = ok_text.into();
    let on_ok = Rc::new(on_ok);

    window.open_dialog(cx, move |dialog, _, cx| {
        let mut props = DialogButtonProps::default()
            .ok_text(ok_text.clone())
            .cancel_text(t!("dialog.cancel").to_string());
        if danger {
            props = props.ok_variant(ButtonVariant::Danger);
        }
        let on_ok = on_ok.clone();
        dialog
            .title(title.clone())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .py_1()
                    .text_sm()
                    .line_height(gpui::relative(1.5))
                    .text_color(cx.theme().muted_foreground)
                    .child(message.clone()),
            )
            .confirm()
            .button_props(props)
            .on_ok(move |_: &ClickEvent, window, cx| {
                on_ok(window, cx);
                true
            })
    });
}

pub fn confirm_delete(
    window: &mut Window,
    cx: &mut App,
    subject: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) {
    let subject = subject.into();
    confirm(
        window,
        cx,
        t!("dialog.delete.title", subject = subject).to_string(),
        detail,
        t!("dialog.delete.ok").to_string(),
        true,
        on_ok,
    );
}

pub fn confirm_discard(
    window: &mut Window,
    cx: &mut App,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) {
    confirm(
        window,
        cx,
        t!("dialog.discard.title").to_string(),
        t!("dialog.discard.message").to_string(),
        t!("dialog.discard.ok").to_string(),
        true,
        on_ok,
    );
}

pub fn confirm_dangerous_run(
    window: &mut Window,
    cx: &mut App,
    summary: impl Into<SharedString>,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) {
    confirm(
        window,
        cx,
        t!("dialog.dangerous.title").to_string(),
        summary,
        t!("dialog.dangerous.ok").to_string(),
        true,
        on_ok,
    );
}

pub fn confirm_import(
    window: &mut Window,
    cx: &mut App,
    source: impl Into<SharedString>,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) {
    let source = source.into();
    confirm(
        window,
        cx,
        t!("dialog.import.title").to_string(),
        t!("dialog.import.message", source = source).to_string(),
        t!("dialog.import.ok").to_string(),
        true,
        on_ok,
    );
}
