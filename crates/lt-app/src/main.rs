#![windows_subsystem = "windows"]

mod app;
mod assets;
mod charts;
mod confirm;
mod forms;
mod panel_auth;
mod panel_history;
mod panel_run;
mod panel_scenarios;
mod ui;

use app::{
    AppView, NewItem, Save, StartRun, StopRun, TabAuth, TabHistory, TabRun, TabScenarios,
};
use assets::Assets;
use gpui::{
    App, AppContext as _, Application, Bounds, KeyBinding, WindowBounds, WindowOptions, px, size,
};
use gpui_component::Root;
use gpui_component::TitleBar;
use gpui_component::theme::{Theme, ThemeMode};
use rust_i18n::t;

rust_i18n::i18n!("locales", fallback = "en");

fn main() {
    Application::new().with_assets(Assets).run(|cx: &mut App| {
        assets::load_fonts(cx);
        gpui_component::init(cx);

        cx.bind_keys([
            KeyBinding::new("cmd-1", TabRun, Some("LoadTesting")),
            KeyBinding::new("cmd-2", TabScenarios, Some("LoadTesting")),
            KeyBinding::new("cmd-3", TabAuth, Some("LoadTesting")),
            KeyBinding::new("cmd-4", TabHistory, Some("LoadTesting")),
            KeyBinding::new("cmd-s", Save, Some("LoadTesting")),
            KeyBinding::new("cmd-enter", StartRun, Some("LoadTesting")),
            KeyBinding::new("cmd-.", StopRun, Some("LoadTesting")),
            KeyBinding::new("cmd-n", NewItem, Some("LoadTesting")),
            // Linux / Windows
            KeyBinding::new("ctrl-1", TabRun, Some("LoadTesting")),
            KeyBinding::new("ctrl-2", TabScenarios, Some("LoadTesting")),
            KeyBinding::new("ctrl-3", TabAuth, Some("LoadTesting")),
            KeyBinding::new("ctrl-4", TabHistory, Some("LoadTesting")),
            KeyBinding::new("ctrl-s", Save, Some("LoadTesting")),
            KeyBinding::new("ctrl-enter", StartRun, Some("LoadTesting")),
            KeyBinding::new("ctrl-.", StopRun, Some("LoadTesting")),
            KeyBinding::new("ctrl-n", NewItem, Some("LoadTesting")),
        ]);

        let settings = lt_core::storage::Settings::load();
        let lang = lt_core::i18n::normalize(&settings.language);
        lt_core::i18n::set_language(lang);
        gpui_component::set_locale(lang);
        let width = settings.window_width.unwrap_or(1280.0).max(960.0);
        let height = settings.window_height.unwrap_or(860.0).max(640.0);
        let bounds = Bounds::centered(None, size(px(width), px(height)), cx);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            },
            |window, cx| {
                match settings.theme.as_str() {
                    "light" => Theme::change(ThemeMode::Light, Some(window), cx),
                    "auto" => Theme::sync_system_appearance(Some(window), cx),
                    _ => Theme::change(ThemeMode::Dark, Some(window), cx),
                };
                ui::apply_theme(cx);
                let view = cx.new(|cx| AppView::new(window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .unwrap_or_else(|error| panic!("{}: {error}", t!("error.open_window")));

        cx.activate(true);
    });
}

#[cfg(test)]
mod tests {
    use rust_i18n::t;

    #[test]
    fn ui_strings_follow_language() {
        lt_core::i18n::set_language("fr");
        assert_eq!(t!("tab.run"), "Lancer");
        assert_eq!(
            t!("toast.folder.added", name = "Prod"),
            "Dossier « Prod » ajouté."
        );
        lt_core::i18n::set_language("en");
        assert_eq!(t!("tab.run"), "Run");
        assert_eq!(
            t!("toast.folder.added", name = "Prod"),
            "Folder “Prod” added."
        );
        lt_core::i18n::set_language("en");
    }
}
