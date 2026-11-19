//! Onglet « Scénarios » : arborescence de dossiers et éditeur structuré.

use crate::app::{AppView, ScenarioEditorTab};
use crate::forms::ScenarioForm;
use crate::ui;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Hsla, IntoElement, ParentElement, SharedString, Styled,
    Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::input::Input;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};
use gpui_component::resizable::{h_resizable, resizable_panel};
use gpui_component::switch::Switch;
use gpui_component::tab::{Tab, TabBar};
use gpui_component::scroll::ScrollableElement as _;
use gpui_component::{ActiveTheme, Disableable, Selectable, Sizable};
use lt_core::models::StepType;
use rust_i18n::t;

impl AppView {
    pub(crate) fn render_scenarios(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_resizable("scenario-layout")
            .child(
                resizable_panel()
                    .size(px(ui::SIDEBAR_WIDTH))
                    .size_range(px(ui::SIDEBAR_MIN_WIDTH)..px(ui::SIDEBAR_MAX_WIDTH))
                    .child(self.render_scenario_sidebar(window, cx)),
            )
            .child(
                resizable_panel().child(self.render_scenario_editor(window, cx)),
            )
            .into_any_element()
    }

    fn render_scenario_sidebar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut tree = div().flex().flex_col().gap_px().p_2();

        let root_tests: Vec<_> = self
            .tests
            .iter()
            .filter(|test| test.folder.is_empty())
            .cloned()
            .collect();
        if !root_tests.is_empty() {
            tree = tree.child(
                div()
                    .px_2()
                    .pt_2()
                    .pb_1()
                    .child(ui::section_label(t!("common.no_folder").to_string(), cx)),
            );
            for test in root_tests {
                tree = tree.child(self.scenario_row(&test, 0, cx));
            }
        }

        let mut collapsed_prefixes: Vec<String> = Vec::new();
        for folder in self.all_folder_paths() {
            if collapsed_prefixes
                .iter()
                .any(|prefix| folder.starts_with(&format!("{prefix}/")))
            {
                continue;
            }
            let depth = folder.matches('/').count();
            let collapsed = self.collapsed.contains(&folder);
            if collapsed {
                collapsed_prefixes.push(folder.clone());
            }
            let label = folder
                .rsplit('/')
                .next()
                .unwrap_or(folder.as_str())
                .to_string();
            let folder_id = folder.clone();
            let chevron = if collapsed {
                "chevron-right"
            } else {
                "chevron-down"
            };
            tree = tree.child(
                ui::list_row(SharedString::from(format!("folder-{folder}")), false, cx)
                    .pl(px(4. + depth as f32 * 14.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        if let Some(index) = this
                            .collapsed
                            .iter()
                            .position(|entry| entry == &folder_id)
                        {
                            this.collapsed.remove(index);
                        } else {
                            this.collapsed.push(folder_id.clone());
                        }
                        this.save_settings();
                        cx.notify();
                    }))
                    .child(
                        ui::icon(chevron)
                            .with_size(gpui_component::Size::Small)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        ui::icon("folder")
                            .with_size(gpui_component::Size::Small)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(label),
                    ),
            );
            if !collapsed {
                let tests: Vec<_> = self
                    .tests
                    .iter()
                    .filter(|test| test.folder == folder)
                    .cloned()
                    .collect();
                for test in tests {
                    tree = tree.child(self.scenario_row(&test, depth + 1, cx));
                }
            }
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
                t!("scenario.header").to_string(),
                Some(
                    Button::new("scenario-new")
                        .xsmall()
                        .primary()
                        .icon(ui::icon("plus"))
                        .label(t!("action.new").to_string())
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                            this.new_test(window, cx)
                        })),
                ),
                cx,
            ))
            .child(
                div()
                    .id("scenario-tree")
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .child(tree),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .py_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(Input::new(&self.new_folder).small().flex_1())
                    .child(
                        Button::new("folder-add")
                            .small()
                            .outline()
                            .icon(ui::icon("folder"))
                            .tooltip(t!("action.create_folder").to_string())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.add_folder(window, cx)
                            })),
                    ),
            )
            .into_any_element()
    }

    fn scenario_row(
        &self,
        test: &lt_core::Test,
        depth: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.selected_test.as_deref() == Some(test.id.as_str());
        let dirty = selected && self.is_scenario_dirty(cx);
        let id = SharedString::from(format!("scenario-row-{}", test.id));
        let test_id = test.id.clone();
        ui::list_row(id, selected, cx)
            .pl(px(6. + depth as f32 * 14.))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.request_select_test(&test_id, window, cx);
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
                    .child(test.name.clone()),
            )
            .when(dirty, |el| {
                el.child(
                    div()
                        .size(px(6.))
                        .flex_none()
                        .rounded_full()
                        .bg(cx.theme().warning),
                )
            })
            .child(ui::mono_muted(format!("{}", test.steps.len()), cx))
            .into_any_element()
    }

    pub(crate) fn all_folder_paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = Vec::new();
        let mut raw: Vec<String> = self.folders.clone();
        raw.extend(self.tests.iter().map(|test| test.folder.clone()));
        for folder in raw {
            let normalized = lt_core::folders::norm_folder(&folder);
            if normalized.is_empty() {
                continue;
            }
            let mut path = String::new();
            for part in normalized.split('/') {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(part);
                if !paths.contains(&path) {
                    paths.push(path.clone());
                }
            }
        }
        paths.sort();
        paths
    }

    fn render_scenario_editor(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(form) = self.scenario.as_ref() else {
            let cta = Button::new("scenario-empty-new")
                .primary()
                .icon(ui::icon("plus"))
                .label(t!("action.create_scenario").to_string())
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                    this.new_test(window, cx)
                }));
            return div()
                .flex()
                .flex_1()
                .size_full()
                .items_center()
                .justify_center()
                .child(ui::empty_state_rich(
                    Some("file"),
                    t!("scenario.empty.title").to_string(),
                    Some(t!("scenario.empty.subtitle").to_string()),
                    Some(cta),
                    cx,
                ))
                .into_any_element();
        };

        let dirty = self.is_scenario_dirty(cx);
        let selected_tab = match self.scenario_tab {
            ScenarioEditorTab::General => 0,
            ScenarioEditorTab::Steps => 1,
            ScenarioEditorTab::Advanced => 2,
            ScenarioEditorTab::Curl => 3,
        };
        let entity = cx.entity().downgrade();

        let folder_crumb = {
            let folder = form.folder.read(cx).value().trim().to_string();
            if folder.is_empty() {
                t!("common.no_folder").to_string()
            } else {
                lt_core::folders::folder_label(&folder)
            }
        };
        let toolbar = ui::toolbar(cx)
            .bg(cx.theme().sidebar)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1p5()
                    .flex_1()
                    .min_w(px(0.))
                    .child(
                        div()
                            .flex_none()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(folder_crumb),
                    )
                    .child(
                        ui::icon("chevron-right")
                            .with_size(gpui_component::Size::XSmall)
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(Input::new(&form.name).small().flex_1()),
            )
            .when(dirty, |el| {
                el.child(ui::chip(t!("chip.modified").to_string(), cx.theme().warning))
            })
            .child(
                Button::new("scenario-duplicate")
                    .small()
                    .ghost()
                    .icon(ui::icon("copy"))
                    .tooltip(t!("action.duplicate").to_string())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.duplicate_test(window, cx)
                    })),
            )
            .child(
                Button::new("scenario-delete")
                    .small()
                    .ghost()
                    .icon(ui::icon("delete"))
                    .tooltip(t!("action.delete").to_string())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.request_delete_test(window, cx)
                    })),
            )
            .child(
                Button::new("scenario-save")
                    .small()
                    .primary()
                    .label(t!("action.save").to_string())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                        this.save_scenario(window, cx)
                    })),
            );

        let tabs = TabBar::new("scenario-editor-tabs")
            .underline()
            .px_3()
            .selected_index(selected_tab)
            .child(Tab::new().label(t!("scenario.tab.general").to_string()))
            .child(Tab::new().label(
                t!("scenario.tab.steps", count = form.steps.len()).to_string(),
            ))
            .child(Tab::new().label(t!("scenario.tab.advanced").to_string()))
            .child(Tab::new().label(t!("scenario.tab.curl").to_string()))
            .on_click(move |index, _window, cx| {
                let tab = match *index {
                    1 => ScenarioEditorTab::Steps,
                    2 => ScenarioEditorTab::Advanced,
                    3 => ScenarioEditorTab::Curl,
                    _ => ScenarioEditorTab::General,
                };
                let _ = entity.update(cx, |this, cx| {
                    this.scenario_tab = tab;
                    cx.notify();
                });
            });

        let body = match self.scenario_tab {
            ScenarioEditorTab::General => self.general_card(form, cx),
            ScenarioEditorTab::Steps => div()
                .flex()
                .flex_col()
                .gap_4()
                .child(self.steps_card(form, cx))
                .child(self.step_editor(form, cx))
                .child(self.try_result_panel(cx))
                .into_any_element(),
            ScenarioEditorTab::Advanced => self.advanced_card(form, cx),
            ScenarioEditorTab::Curl => self.curl_card(form, cx),
        };

        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.))
            .min_h(px(0.))
            .child(toolbar)
            .child(tabs)
            .child(
                div()
                    .id("scenario-editor")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(0.))
                    .overflow_y_scroll()
                    .p_4()
                    .gap_4()
                    .child(body),
            )
            .into_any_element()
    }

    fn general_card(&self, form: &ScenarioForm, cx: &mut Context<Self>) -> AnyElement {
        let folder_value = form.folder.read(cx).value().to_string();
        let folder_label = if folder_value.trim().is_empty() {
            t!("common.no_folder").to_string()
        } else {
            folder_value.clone()
        };
        let folders = self.all_folder_paths();
        let weak = cx.entity().downgrade();
        let folder_menu = ui::select_button("folder-select", folder_label)
            .dropdown_menu({
                let weak = weak.clone();
                let current = folder_value.clone();
                move |menu, _window, _cx| {
                    let mut menu = menu.item({
                        let weak = weak.clone();
                        PopupMenuItem::new(t!("common.no_folder").to_string())
                            .checked(current.trim().is_empty())
                            .on_click(move |_, window, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if let Some(form) = this.scenario.as_mut() {
                                        form.folder.update(cx, |state, cx| {
                                            state.set_value("", window, cx);
                                        });
                                    }
                                    cx.notify();
                                });
                            })
                    });
                    for folder in &folders {
                        let checked = current == *folder;
                        let folder_clone = folder.clone();
                        let weak = weak.clone();
                        menu = menu.item(
                            PopupMenuItem::new(folder.clone())
                                .checked(checked)
                                .on_click(move |_, window, cx| {
                                    let value = folder_clone.clone();
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Some(form) = this.scenario.as_mut() {
                                            form.folder.update(cx, |state, cx| {
                                                state.set_value(value.clone(), window, cx);
                                            });
                                        }
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                }
            });

        let auth_label = if form.auth_id.is_empty() {
            t!("common.none").to_string()
        } else {
            self.auths
                .iter()
                .find(|auth| auth.id == form.auth_id)
                .map(|auth| auth.name.clone())
                .unwrap_or_else(|| t!("common.unknown").to_string())
        };
        let auths = self.auths.clone();
        let current_auth = form.auth_id.clone();
        let auth_menu = ui::select_button("auth-select", auth_label)
            .dropdown_menu({
                let weak = weak.clone();
                move |menu, _window, _cx| {
                    let mut menu = menu.item({
                        let weak = weak.clone();
                        PopupMenuItem::new(t!("common.none").to_string())
                            .checked(current_auth.is_empty())
                            .on_click(move |_, _, cx| {
                                let _ = weak.update(cx, |this, cx| {
                                    if let Some(form) = this.scenario.as_mut() {
                                        form.auth_id.clear();
                                    }
                                    cx.notify();
                                });
                            })
                    });
                    for auth in &auths {
                        let checked = current_auth == auth.id;
                        let auth_id = auth.id.clone();
                        let weak = weak.clone();
                        menu = menu.item(
                            PopupMenuItem::new(auth.name.clone())
                                .checked(checked)
                                .on_click(move |_, _, cx| {
                                    let auth_id = auth_id.clone();
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Some(form) = this.scenario.as_mut() {
                                            form.auth_id = auth_id;
                                        }
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                }
            });

        ui::card(cx)
            .flex()
            .flex_col()
            .gap_3()
            .child(ui::section_title(t!("scenario.general").to_string(), cx))
            .child(ui::row(vec![
                ui::select_field(&t!("folder.field"), folder_menu, cx),
                ui::field(&t!("scenario.field.base_url"), &form.base_url, cx),
            ]))
            .child(ui::area(
                &t!("scenario.field.description"),
                &form.description,
                cx,
            ))
            .child(ui::select_field(
                &t!("scenario.field.authentication"),
                auth_menu,
                cx,
            ))
            .into_any_element()
    }

    fn advanced_card(&self, form: &ScenarioForm, cx: &mut Context<Self>) -> AnyElement {
        ui::card(cx)
            .flex()
            .flex_col()
            .gap_3()
            .child(ui::section_title(t!("scenario.advanced").to_string(), cx))
            .child(ui::row(vec![
                ui::field(&t!("scenario.field.think_time"), &form.think_time, cx),
                ui::field(&t!("scenario.field.threshold_p95"), &form.p95, cx),
                ui::field(&t!("scenario.field.threshold_p99"), &form.p99, cx),
                ui::field(&t!("scenario.field.threshold_errors"), &form.error_rate, cx),
                ui::field(&t!("scenario.field.threshold_checks"), &form.checks_rate, cx),
            ]))
            .child(ui::area(
                &t!("scenario.field.variables"),
                &form.variables,
                cx,
            ))
            .child(ui::divider(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                Switch::new("insecure-tls")
                                    .checked(form.insecure_tls)
                                    .label(t!("scenario.insecure_tls").to_string())
                                    .on_click(cx.listener(
                                        |this, checked: &bool, _window, cx| {
                                            if let Some(form) = this.scenario.as_mut() {
                                                form.insecure_tls = *checked;
                                            }
                                            cx.notify();
                                        },
                                    )),
                            )
                            .child(ui::muted(t!("scenario.insecure_tls_hint").to_string(), cx)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_3()
                            .child(
                                Switch::new("dangerous")
                                    .checked(form.dangerous)
                                    .label(t!("scenario.dangerous").to_string())
                                    .on_click(cx.listener(
                                        |this, checked: &bool, _window, cx| {
                                            if let Some(form) = this.scenario.as_mut() {
                                                form.dangerous = *checked;
                                            }
                                            cx.notify();
                                        },
                                    )),
                            )
                            .child(ui::muted(t!("scenario.dangerous_hint").to_string(), cx)),
                    ),
            )
            .into_any_element()
    }

    fn curl_card(&self, form: &ScenarioForm, cx: &mut Context<Self>) -> AnyElement {
        ui::card(cx)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(ui::section_title(t!("scenario.curl.title").to_string(), cx))
                    .child(ui::muted(t!("scenario.curl.hint").to_string(), cx)),
            )
            .child(Input::new(&form.curl).w_full())
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("curl-import")
                            .small()
                            .outline()
                            .label(t!("action.import_as_step").to_string())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.import_curl(window, cx)
                            })),
                    )
                    .child(ui::muted(t!("scenario.curl.base_url_hint").to_string(), cx)),
            )
            .into_any_element()
    }

    fn steps_card(&self, form: &ScenarioForm, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div().flex().flex_col().gap_px();
        for (index, step) in form.steps.iter().enumerate() {
            let selected = form.selected_step == index;
            let name = step.name.read(cx).value().to_string();
            let path = step.path.read(cx).value().to_string();
            let method = if step.step_type == StepType::Soap {
                "SOAP".to_string()
            } else {
                step.method.to_uppercase()
            };
            let color = method_color(&method, cx);
            let path_label = if path.trim().is_empty() {
                "/".to_string()
            } else {
                path
            };
            list = list.child(
                ui::list_row(SharedString::from(format!("step-row-{index}")), selected, cx)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _window, cx| {
                        if let Some(form) = this.scenario.as_mut() {
                            form.selected_step = index;
                        }
                        this.try_result = None;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .w(px(18.))
                            .flex_none()
                            .child(ui::mono_muted(format!("{}", index + 1), cx)),
                    )
                    .child(
                        div()
                            .w(px(48.))
                            .flex_none()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_xs()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(color)
                            .child(method),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .font_family(cx.theme().mono_font_family.clone())
                            .text_xs()
                            .child(path_label),
                    )
                    .when(!name.trim().is_empty(), |el| {
                        el.child(
                            div()
                                .max_w(px(160.))
                                .truncate()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(name.clone()),
                        )
                    })
                    .child(
                        Button::new(SharedString::from(format!("step-remove-{index}")))
                            .xsmall()
                            .ghost()
                            .icon(ui::icon("delete"))
                            .tooltip(t!("action.remove_step").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.request_delete_step(index, window, cx)
                            })),
                    ),
            );
        }
        if form.steps.is_empty() {
            list = list.child(ui::muted(t!("step.empty").to_string(), cx));
        }

        ui::card(cx)
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(ui::section_label(t!("step.header").to_string(), cx))
                    .child(ui::mono_muted(format!("{}", form.steps.len()), cx))
                    .child(div().flex_1())
                    .child(
                        Button::new("step-add")
                            .xsmall()
                            .outline()
                            .icon(ui::icon("plus"))
                            .label(t!("action.add_step").to_string())
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                                this.add_step(window, cx)
                            })),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    fn step_editor(
        &self,
        form: &ScenarioForm,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(step) = form.selected_step() else {
            return div().into_any_element();
        };
        let index = form.selected_step;
        let is_rest = step.step_type == StepType::Rest;

        let mut types = div().flex().flex_row().gap_1();
        for step_type in [StepType::Rest, StepType::Soap] {
            let selected = step.step_type == step_type;
            let label = if step_type == StepType::Rest {
                "REST"
            } else {
                "SOAP"
            };
            types = types.child(
                Button::new(SharedString::from(format!(
                    "step-type-{}",
                    step_type.as_str()
                )))
                .xsmall()
                .outline()
                .selected(selected)
                .label(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.set_step_type(index, step_type, window, cx)
                })),
            );
        }

        // Sélecteur de méthode (menu déroulant) pour les étapes REST.
        let weak = cx.entity().downgrade();
        let current_method = step.method.to_uppercase();
        let method_btn = Button::new(SharedString::from(format!("step-method-{index}")))
            .outline()
            .dropdown_caret(true)
            .label(current_method.clone())
            .dropdown_menu({
                let current_method = current_method.clone();
                move |menu, _window, _cx| {
                    let mut menu = menu;
                    for method in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
                        let weak = weak.clone();
                        let value = method.to_string();
                        menu = menu.item(
                            PopupMenuItem::new(method)
                                .checked(current_method == method)
                                .on_click(move |_, _, cx| {
                                    let value = value.clone();
                                    let _ = weak.update(cx, |this, cx| {
                                        if let Some(form) = this.scenario.as_mut()
                                            && let Some(step) = form.steps.get_mut(index)
                                        {
                                            step.method = value;
                                        }
                                        cx.notify();
                                    });
                                }),
                        );
                    }
                    menu
                }
            });

        let mut soap_versions = div().flex().flex_row().gap_1();
        for version in ["1.1", "1.2"] {
            let selected = step.soap_version == version;
            let value = version.to_string();
            soap_versions = soap_versions.child(
                Button::new(SharedString::from(format!("soap-version-{version}")))
                    .xsmall()
                    .outline()
                    .selected(selected)
                    .label(format!("SOAP {version}"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.set_soap_version(index, &value, window, cx);
                    })),
            );
        }

        let step_label = {
            let name = step.name.read(cx).value().trim().to_string();
            if name.is_empty() {
                if step.step_type == StepType::Soap {
                    "SOAP".to_string()
                } else {
                    step.method.to_uppercase()
                }
            } else {
                name
            }
        };

        let try_btn = Button::new("step-try")
            .primary()
            .icon(ui::icon("play"))
            .label(SharedString::from(if self.try_running {
                t!("action.testing").to_string()
            } else {
                t!("action.test").to_string()
            }))
            .tooltip(t!("action.test_request").to_string())
            .disabled(self.try_running)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.try_step(window, cx)
            }));

        // Ligne façon client HTTP : méthode + chemin + Tester.
        let http_row = div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .child(if is_rest {
                method_btn.into_any_element()
            } else {
                div()
                    .flex()
                    .items_center()
                    .h(px(32.))
                    .px_2()
                    .rounded(cx.theme().radius)
                    .bg(cx.theme().info.opacity(0.12))
                    .font_family(cx.theme().mono_font_family.clone())
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(cx.theme().info)
                    .child("SOAP")
                    .into_any_element()
            })
            .child(Input::new(&step.path).flex_1())
            .child(try_btn);

        let mut card = ui::card(cx)
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(ui::section_label(
                        t!("step.editor_title", index = index + 1).to_string(),
                        cx,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .truncate()
                            .text_sm()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(step_label),
                    )
                    .child(types),
            )
            .child(http_row)
            .child(ui::field(&t!("step.field.name"), &step.name, cx));

        if !is_rest {
            card = card
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(ui::field_label_row(&t!("step.field.soap_version"), None, cx))
                        .child(soap_versions),
                )
                .child(ui::field(&t!("step.field.soap_action"), &step.soap_action, cx))
                .child(ui::muted(t!("step.soap_hint").to_string(), cx));
        }

        card = card
            .child(ui::row(vec![
                ui::field(
                    &t!("step.field.expected_status"),
                    &step.expected_status,
                    cx,
                ),
                ui::field(&t!("step.field.p95"), &step.p95, cx),
            ]))
            .child(ui::area(&t!("step.field.headers"), &step.headers, cx))
            .child(ui::area(&t!("step.field.body"), &step.body, cx))
            .child(ui::row(vec![
                ui::area(&t!("step.field.checks"), &step.checks, cx),
                ui::area(&t!("step.field.extract"), &step.extract, cx),
            ]))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_3()
                    .child(
                        Switch::new(SharedString::from(format!("step-stop-{index}")))
                            .checked(step.stop_on_failure)
                            .label(t!("step.stop_on_failure").to_string())
                            .on_click(cx.listener(move |this, checked: &bool, _window, cx| {
                                if let Some(form) = this.scenario.as_mut()
                                    && let Some(step) = form.steps.get_mut(index)
                                {
                                    step.stop_on_failure = *checked;
                                }
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("step-duplicate")
                            .small()
                            .ghost()
                            .icon(ui::icon("copy"))
                            .label(t!("action.duplicate").to_string())
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                                this.duplicate_step(index, window, cx)
                            })),
                    ),
            );

        card.into_any_element()
    }

    fn try_result_panel(&self, cx: &App) -> AnyElement {
        if self.try_running {
            return ui::surface(cx)
                .p_3()
                .flex()
                .flex_col()
                .gap_2()
                .child(ui::section_label(t!("step.result.title").to_string(), cx))
                .child(ui::muted(t!("step.result.running").to_string(), cx))
                .into_any_element();
        }

        let Some(result) = self.try_result.as_ref() else {
            return div().into_any_element();
        };

        ui::surface(cx)
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(ui::section_label(t!("step.result.title").to_string(), cx))
            .child(try_result_elements(result, &self.try_body_scroll, cx))
            .into_any_element()
    }
}

/// Couleur associée à une méthode HTTP (GET vert, POST violet, PUT orange, DELETE rouge).
fn method_color(method: &str, cx: &App) -> Hsla {
    match method.to_uppercase().as_str() {
        "GET" => cx.theme().success,
        "POST" => cx.theme().primary,
        "PUT" | "PATCH" => cx.theme().warning,
        "DELETE" => cx.theme().danger,
        "SOAP" => cx.theme().info,
        _ => cx.theme().muted_foreground,
    }
}

fn try_result_elements(
    result: &Result<lt_engine::TryResult, String>,
    body_scroll: &gpui::ScrollHandle,
    cx: &App,
) -> AnyElement {
    match result {
        Err(error) => div()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .rounded(cx.theme().radius)
            .bg(cx.theme().danger.opacity(0.12))
            .font_family(cx.theme().mono_font_family.clone())
            .text_xs()
            .text_color(cx.theme().danger)
            .child(error.clone())
            .into_any_element(),
        Ok(result) => {
            let status = result.response.status;
            let color = if result.ok {
                cx.theme().success
            } else {
                cx.theme().danger
            };
            let mut checks = div().flex().flex_col().gap_1();
            for check in &result.checks {
                checks = checks.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .text_xs()
                        .child(
                            div()
                                .text_color(if check.ok {
                                    cx.theme().success
                                } else {
                                    cx.theme().danger
                                })
                                .child(if check.ok { "✓" } else { "✗" }),
                        )
                        .child(div().child(check.label.clone())),
                );
            }
            if result.checks.is_empty() {
                checks = checks.child(ui::muted(t!("step.result.no_checks").to_string(), cx));
            }

            let mut headers = div().flex().flex_col().gap_0p5();
            for (key, value) in result.display_headers() {
                headers = headers.child(
                    div()
                        .text_xs()
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{key}: {value}")),
                );
            }

            let body_preview = if result.response.body_text.chars().count() > 8000 {
                let mut preview: String = result.response.body_text.chars().take(8000).collect();
                preview.push('…');
                preview
            } else {
                result.response.body_text.clone()
            };

            let mut body_lines = div()
                .flex()
                .flex_col()
                .gap_0p5()
                .w_full()
                .min_w(px(0.))
                .font_family(cx.theme().mono_font_family.clone())
                .text_xs();
            if body_preview.trim().is_empty() {
                body_lines = body_lines.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("common.empty").to_string()),
                );
            } else {
                for line in body_preview.lines() {
                    let line = if line.is_empty() { " " } else { line };
                    body_lines = body_lines.child(
                        div()
                            .w_full()
                            .min_w(px(0.))
                            .whitespace_nowrap()
                            .child(line.to_string()),
                    );
                }
            }

            let mut panel = div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .child(ui::chip(
                            status
                                .map(|code| code.to_string())
                                .unwrap_or_else(|| "—".to_string()),
                            color,
                        ))
                        .child(ui::mono(
                            format!("{:.0} ms", result.response.duration_ms),
                            cx,
                        ))
                        .child(div().flex_1())
                        .child(ui::chip(
                            if result.ok {
                                t!("common.ok").to_string()
                            } else {
                                t!("common.failure").to_string()
                            },
                            color,
                        )),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_2()
                        .child(
                            div()
                                .font_family(cx.theme().mono_font_family.clone())
                                .text_xs()
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(method_color(&result.request.method, cx))
                                .child(result.request.method.to_string()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.))
                                .truncate()
                                .child(ui::mono_muted(result.request.url.to_string(), cx)),
                        ),
                );

            if let Some(error) = &result.response.error {
                panel = panel.child(
                    div()
                        .p_2()
                        .rounded(cx.theme().radius)
                        .bg(cx.theme().danger.opacity(0.12))
                        .font_family(cx.theme().mono_font_family.clone())
                        .text_xs()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                );
            }

            panel
                .child(ui::divider(cx))
                .child(ui::section_label(t!("step.result.sent_headers").to_string(), cx))
                .child(headers)
                .child(ui::section_label(t!("step.result.checks").to_string(), cx))
                .child(checks)
                .child(ui::section_label(t!("step.result.response_body").to_string(), cx))
                .child(
                    div()
                        .id("try-response-inline")
                        .relative()
                        .h(px(280.))
                        .w_full()
                        .min_w(px(0.))
                        .min_h(px(0.))
                        .overflow_scroll()
                        .track_scroll(body_scroll)
                        .vertical_scrollbar(body_scroll)
                        .p_2()
                        .rounded(cx.theme().radius)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(crate::panel_run::terminal_bg(cx))
                        .child(body_lines),
                )
                .into_any_element()
        }
    }
}
