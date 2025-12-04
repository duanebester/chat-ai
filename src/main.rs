mod assets;
mod theme;
mod window;

use gpui::{
    AppContext as _, Application, ClickEvent, Context, Entity, IntoElement, KeyBinding,
    ParentElement as _, Render, Styled as _, Window, actions, div, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, Root, Sizable as _, StyledExt as _, ThemeMode, TitleBar,
    button::*,
    divider::Divider,
    h_flex,
    input::{Input, InputState},
    label::Label,
    menu::{DropdownMenu, PopupMenuItem},
};

use crate::{
    assets::Assets,
    theme::change_color_mode,
    window::{blur_window, get_window_options},
};

actions!(window, [Quit, StandardAction]);

pub struct HelloWorld {
    input: Entity<InputState>,
}

impl HelloWorld {
    pub fn change_mode(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        tracing::debug!("Current mode: {:?}", cx.theme().mode);
        let new_mode = if cx.theme().mode.is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        change_color_mode(new_mode, window, cx);
    }
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme_toggle = Button::new("theme-mode")
            .map(|this| {
                if cx.theme().mode.is_dark() {
                    this.icon(Icon::empty().path("icons/sun.svg"))
                } else {
                    this.icon(Icon::empty().path("icons/moon.svg"))
                }
            })
            .small()
            .ghost()
            .on_click(cx.listener(Self::change_mode));
        let header = TitleBar::new().child(
            h_flex()
                .w_full()
                .p_1()
                .justify_between()
                .child(Label::new("AI Chat"))
                .child(div().pr(px(5.0)).flex().items_center().child(theme_toggle)),
        );

        let form_header = div()
            .flex()
            .gap_1()
            .p_2()
            .justify_start()
            .items_center()
            .child(
                Button::new("add-mention")
                    .icon(Icon::empty().path("icons/at-sign.svg"))
                    .ghost()
                    .mr_1(),
            )
            .child(Divider::vertical())
            .child(Label::new("Intro Call").pl_2());

        let form_footer = div()
            .flex()
            .gap_2()
            .p_2()
            .justify_between()
            .items_center()
            .child(
                div()
                    .flex()
                    .justify_start()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new("attachment")
                            .icon(Icon::empty().path("icons/paperclip.svg"))
                            .ghost()
                            .mr_1(),
                    )
                    .child(Divider::vertical())
                    .child(Button::new("model").label("Auto").ghost().dropdown_menu(
                        |menu, _window, _cx| {
                            menu.item(
                                PopupMenuItem::new("Claude Haiku 4.5")
                                    .disabled(false)
                                    .icon(Icon::empty().path("icons/anthropic.svg"))
                                    .on_click(|_evt, _window, _cx| {
                                        println!("Haiku Action Clicked!");
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Claude Sonnet 4.5")
                                    .disabled(false)
                                    .icon(Icon::empty().path("icons/anthropic.svg"))
                                    .on_click(|_evt, _window, _cx| {
                                        println!("Sonnet Action Clicked!");
                                    }),
                            )
                            .item(
                                PopupMenuItem::new("Claude Opus 4.5")
                                    .disabled(false)
                                    .icon(Icon::empty().path("icons/anthropic.svg"))
                                    .on_click(|_evt, _window, _cx| {
                                        println!("Opus Action Clicked!");
                                    }),
                            )
                        },
                    )),
            )
            .child(
                Button::new("send")
                    .rounded_full()
                    .bg(cx.theme().accent)
                    .icon(Icon::empty().path("icons/move-up.svg")),
            );

        let form = div()
            .flex()
            .flex_col()
            .justify_between()
            .rounded_2xl()
            .border_1()
            .border_color(cx.theme().border.opacity(0.8))
            .bg(cx.theme().popover)
            .min_h(px(160.))
            .shadow_lg()
            .w_full()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(form_header)
                    .child(Input::new(&self.input.clone()).appearance(false)),
            )
            .child(form_footer);

        let empty_content = div()
            .flex()
            .flex_col()
            .flex_grow()
            .justify_end()
            .gap_4()
            .p_4()
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap_2()
                    .justify_start()
                    .items_center()
                    .child(Icon::empty().path("icons/pencil-line.svg"))
                    .child(Label::new("Draft a reply")),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap_2()
                    .justify_start()
                    .items_center()
                    .child(Icon::empty().path("icons/wand-sparkles.svg"))
                    .child(Label::new("Summarize an email")),
            )
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap_2()
                    .justify_start()
                    .items_center()
                    .child(Icon::empty().path("icons/text-select.svg"))
                    .child(Label::new("Extract text")),
            );

        div().v_flex().size_full().child(header).child(
            div()
                .p_2()
                .v_flex()
                .size_full()
                .child(empty_content)
                .child(form),
        )
    }
}

fn main() {
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

                let input = cx.new(|cx| {
                    InputState::new(window, cx).placeholder("Write, or press \"@\" to add context")
                });
                let view = cx.new(|_| HelloWorld { input });
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
