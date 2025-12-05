mod assets;
mod chat;
mod handler;
mod services;
mod theme;
mod window;

use gpui::{AppContext as _, Application, KeyBinding, actions};
use gpui_component::{ActiveTheme as _, Root};

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use crate::{
    assets::Assets,
    chat::ChatAI,
    theme::change_color_mode,
    window::{blur_window, get_window_options},
};

actions!(window, [Quit, StandardAction]);

fn init_logging() {
    // Check for --debug flag or -d
    let debug = std::env::args().any(|arg| arg == "--debug" || arg == "-d");

    // Also respect RUST_LOG env var for fine-grained control
    let filter = if debug {
        EnvFilter::new("debug")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
    };

    tracing_subscriber::registry()
        .with(fmt::layer().with_target(true))
        .with(filter)
        .init();
}

fn main() {
    init_logging();

    // Create app w/ assets
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        // Close app on macOS close icon click
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        let window_opts = get_window_options(cx);
        cx.spawn(async move |cx| {
            cx.open_window(window_opts, |window, cx| {
                blur_window(window);
                // This must be called before using any GPUI Component features.
                gpui_component::init(cx);
                change_color_mode(cx.theme().mode, window, cx);
                let view = ChatAI::view(window, cx);
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();

        // Close app w/ cmd-q
        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);

        // Bring app to front
        cx.activate(true);
    });
}
