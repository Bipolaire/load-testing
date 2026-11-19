//! Onglet « Auth » : gestion des authentifications (OAuth2, Bearer, clé d'API).

use crate::app::AppView;
use crate::ui;
use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, IntoElement, ParentElement, SharedString, Styled, Window, div,
    px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::{ActiveTheme, Sizable};
use lt_core::models::{Auth, AuthType};
use rust_i18n::t;

impl AppView {
    pub(crate) fn render_auth(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_resizable("auth-layout")
            .child(
                resizable_panel()
                    .size(px(ui::SIDEBAR_WIDTH))
                    .size_range(px(ui::SIDEBAR_MIN_WIDTH)..px(ui::SIDEBAR_MAX_WIDTH))
                    .child(self.render_auth_list(cx)),
            )
            .child(resizable_panel().child(self.render_auth_editor(cx)))
            .into_any_element()
    }

    fn render_auth_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap_px().p_2();
        if self.auths.is_empty() {
            list = list.child(
                div()
                    .p_3()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(t!("auth.empty_list").to_string()),
            );
        }
        for auth in &self.auths {
            let selected = self.selected_auth.as_deref() == Some(auth.id.as_str());
            let usage = self
                .tests
                .iter()
                .filter(|test| test.auth_id == auth.id)
                .count();
            let id = SharedString::from(format!("auth-pick-{}", auth.id));
            let auth_id = auth.id.clone();
            list = list.child(
                ui::list_row(id, selected, cx)
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.request_select_auth(&auth_id, window, cx);
                    }))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_sm()
                            .font_weight(if selected {
                                gpui::FontWeight::MEDIUM
                            } else {
                                gpui::FontWeight::NORMAL
                            })
                            .child(auth.name.clone()),
                    )
                    .when(usage > 0, |el| {
                        el.child(ui::mono_muted(format!("{usage}"), cx))
                    })
                    .child(ui::chip(short_type_label(auth.auth_type), cx.theme().muted_foreground)),
            );
        }

        div()
            .flex()
            .flex_col()
            .size_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().sidebar)
            .child(self.render_sidebar_nav(cx))
            .child(ui::sidebar_header(
                t!("auth.header").to_string(),
                Some(
                    Button::new("auth-new")
                        .xsmall()
                        .primary()
                        .icon(ui::icon("plus"))
                        .label(t!("action.new_feminine").to_string())
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.new_auth(window, cx)
                        })),
                ),
                cx,
            ))
            .child(
                div()
                    .id("auth-list")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(list),
            )
            .into_any_element()
    }

    fn render_auth_editor(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(form) = self.auth_form.as_ref() else {
            let cta = Button::new("auth-empty-new")
                .primary()
                .icon(ui::icon("plus"))
                .label(t!("action.add_auth").to_string())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.new_auth(window, cx)
                }));
            return div()
                .flex()
                .flex_1()
                .size_full()
                .items_center()
                .justify_center()
                .child(ui::empty_state_rich(
                    Some("settings"),
                    t!("auth.empty.title").to_string(),
                    Some(t!("auth.empty.subtitle").to_string()),
                    Some(cta),
                    cx,
                ))
                .into_any_element();
        };

        let weak = cx.entity().downgrade();
        let current_type = form.auth_type;
        let type_menu = ui::select_button("auth-type-select", form.auth_type.label())
            .dropdown_menu({
                let weak = weak.clone();
                move |menu, _window, _cx| {
                    let mut menu = menu;
                    for auth_type in AuthType::ALL {
                        let checked = current_type == auth_type;
                        let weak = weak.clone();
                        menu = menu.item(
                            PopupMenuItem::new(auth_type.label())
                                .checked(checked)
                                .on_click(move |_, _, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Some(form) = this.auth_form.as_mut() {
                                            form.auth_type = auth_type;
                                        }
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                }
            });

        let mut fields = div().flex().flex_col().gap_3();
        fields = fields.child(ui::row(vec![
            ui::field(&t!("common.name"), &form.name, cx),
            ui::select_field(&t!("common.type"), type_menu, cx),
        ]));

        match form.auth_type {
            AuthType::None => {
                fields = fields.child(ui::muted(t!("auth.none_hint").to_string(), cx));
            }
            AuthType::OAuth2 => {
                let current_grant = form.grant.clone();
                let grant_label = if form.grant == "password" {
                    t!("auth.grant.password").to_string()
                } else {
                    t!("auth.grant.client_credentials").to_string()
                };
                let grant_menu = ui::select_button("grant-select", grant_label)
                    .dropdown_menu({
                        let weak = weak.clone();
                        move |menu, _window, _cx| {
                            menu.item({
                                let weak = weak.clone();
                                PopupMenuItem::new(t!("auth.grant.client_credentials").to_string())
                                    .checked(current_grant != "password")
                                    .on_click(move |_, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            if let Some(form) = this.auth_form.as_mut() {
                                                form.grant = "client_credentials".to_string();
                                            }
                                            cx.notify();
                                        });
                                    })
                            })
                            .item({
                                let weak = weak.clone();
                                PopupMenuItem::new(t!("auth.grant.password").to_string())
                                    .checked(current_grant == "password")
                                    .on_click(move |_, _, cx| {
                                        let _ = weak.update(cx, |this, cx| {
                                            if let Some(form) = this.auth_form.as_mut() {
                                                form.grant = "password".to_string();
                                            }
                                            cx.notify();
                                        });
                                    })
                            })
                        }
                    });
                fields = fields.child(ui::row(vec![
                    ui::field(&t!("auth.field.token_url"), &form.token_url, cx),
                    ui::select_field(&t!("auth.field.grant_type"), grant_menu, cx),
                ]));
                fields = fields.child(ui::row(vec![
                    ui::field("client_id", &form.client_id, cx),
                    ui::field("client_secret", &form.client_secret, cx),
                ]));
                fields = fields.child(ui::field(
                    &t!("auth.field.client_basic"),
                    &form.client_basic,
                    cx,
                ));
                if form.grant == "password" {
                    fields = fields.child(ui::row(vec![
                        ui::field(&t!("auth.field.username"), &form.username, cx),
                        ui::field(&t!("auth.field.password"), &form.password, cx),
                    ]));
                }
            }
            AuthType::Bearer => {
                fields = fields.child(ui::field(&t!("auth.field.bearer_token"), &form.token, cx));
            }
            AuthType::Apikey => {
                fields = fields.child(ui::row(vec![
                    ui::field(&t!("auth.field.header"), &form.header_name, cx),
                    ui::field(&t!("auth.field.api_key"), &form.api_key, cx),
                ]));
            }
        }

        let title = if form.name.read(cx).value().is_empty() {
            t!("auth.default_title").to_string()
        } else {
            form.name.read(cx).value().to_string()
        };

        div()
            .id("auth-editor")
            .flex()
            .flex_col()
            .size_full()
            .min_w(px(0.))
            .min_h(px(0.))
            .overflow_y_scroll()
            .child(
                ui::toolbar(cx)
                    .bg(cx.theme().sidebar)
                    .child(
                        ui::icon("key-round")
                            .with_size(gpui_component::Size::Small)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(ui::chip(form.auth_type.label(), cx.theme().primary))
                    .child(
                        Button::new("auth-delete")
                            .small()
                            .ghost()
                            .icon(ui::icon("delete"))
                            .label(t!("action.delete").to_string())
                            .text_color(cx.theme().danger)
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.request_delete_auth(window, cx)
                            })),
                    )
                    .child(
                        Button::new("auth-save")
                            .small()
                            .primary()
                            .label(t!("action.save").to_string())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.save_auth(window, cx)
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .p_4()
                    .child(
                        ui::surface(cx)
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(ui::section_label(t!("auth.configuration").to_string(), cx))
                            .child(fields),
                    )
                    .child(ui::muted(t!("auth.secrets_hint").to_string(), cx)),
            )
            .into_any_element()
    }
}

/// Libellé de type sans la précision entre parenthèses, pour la liste latérale.
fn short_type_label(auth_type: AuthType) -> String {
    let label = auth_type.label();
    match label.split_once(" (") {
        Some((short, _)) => short.to_string(),
        None => label,
    }
}

/// Rendu du formulaire pour une authentification existante (réexport utilitaire).
pub(crate) fn new_auth(base: &[Auth]) -> Auth {
    let mut auth = Auth {
        id: lt_core::storage::new_id(),
        ..Default::default()
    };
    let base_name = auth.name.clone();
    let mut index = 2;
    while base.iter().any(|existing| existing.name == auth.name) {
        auth.name = format!("{base_name} {index}");
        index += 1;
    }
    auth
}
