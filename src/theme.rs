use std::{collections::HashMap, rc::Rc, sync::LazyLock};

use gpui::{App, SharedString, Window};
use gpui_component::{Theme, ThemeConfig, ThemeMode, ThemeSet};

pub static THEMES: LazyLock<HashMap<SharedString, ThemeConfig>> = LazyLock::new(|| {
    fn parse_themes(source: &str) -> ThemeSet {
        serde_json::from_str(source).unwrap()
    }

    let mut themes = HashMap::new();
    for source in [
        include_str!("../themes/ayu.json"),
        include_str!("../themes/catppuccin.json"),
    ] {
        let theme_set = parse_themes(source);
        for theme in theme_set.themes {
            themes.insert(theme.name.clone(), theme);
        }
    }

    themes
});

pub fn change_color_mode(mode: ThemeMode, _win: &mut Window, cx: &mut App) {
    let theme_name = match mode {
        ThemeMode::Light => "Catppuccin Latte",
        ThemeMode::Dark => "Catppuccin Macchiato",
    };

    if let Some(theme_config) = THEMES.get(theme_name) {
        let theme_config = Rc::new(theme_config.clone());
        let theme = Theme::global_mut(cx);

        theme.apply_config(&theme_config);
        theme.colors.background = theme.colors.background.opacity(0.85);

        // This doesn't work, maybe b/c of RenderOnce?
        theme.colors.title_bar = theme.colors.title_bar.opacity(0.85);
    }
}
