use gpui::*;
use gpui_component::TitleBar;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
#[serde(rename_all = "lowercase", tag = "type")]
pub enum WindowBackgroundAppearanceContent {
    Blurred {
        opacity: f32,
    },
    Transparent {
        opacity: f32,
    },
    #[default]
    Opaque,
}

impl From<WindowBackgroundAppearanceContent> for WindowBackgroundAppearance {
    fn from(content: WindowBackgroundAppearanceContent) -> Self {
        match content {
            WindowBackgroundAppearanceContent::Blurred { .. } => {
                WindowBackgroundAppearance::Blurred
            }
            WindowBackgroundAppearanceContent::Transparent { .. } => {
                WindowBackgroundAppearance::Transparent
            }
            WindowBackgroundAppearanceContent::Opaque => WindowBackgroundAppearance::Opaque,
        }
    }
}

impl WindowBackgroundAppearanceContent {
    pub fn opacity(&self) -> f32 {
        match self {
            WindowBackgroundAppearanceContent::Blurred { opacity }
            | WindowBackgroundAppearanceContent::Transparent { opacity } => *opacity,
            WindowBackgroundAppearanceContent::Opaque => 1.0,
        }
    }
}

pub fn get_window_options(app: &mut App) -> WindowOptions {
    let display_id_maybe = app.displays().last().map(|d| d.id());
    let bounds = Bounds::centered(display_id_maybe, size(px(400.0), px(600.0)), app);
    WindowOptions {
        display_id: display_id_maybe,
        is_movable: true,
        kind: WindowKind::Normal,
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitleBar::title_bar_options()),
        window_decorations: Some(WindowDecorations::Client),
        ..Default::default()
    }
}

pub fn blur_window(window: &mut Window) {
    window.set_background_appearance(WindowBackgroundAppearance::from(
        WindowBackgroundAppearanceContent::Blurred { opacity: 0.9 },
    ));
}
