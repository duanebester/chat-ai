mod assets;
mod handler;
mod services;
mod theme;
mod window;

use async_channel::{Sender, unbounded};
use gpui::{
    AnyElement, AppContext as _, Application, ClickEvent, Context, Div, Entity, IntoElement,
    KeyBinding, ListAlignment, ListState, ParentElement as _, Render, SharedString, Styled as _,
    Window, actions, div, list, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IndexPath, Root, Sizable as _, StyledExt as _, ThemeMode, TitleBar,
    button::*,
    divider::Divider,
    h_flex,
    input::{Input, InputState},
    label::Label,
    select::{Select, SelectEvent, SelectState},
    text::TextView,
};

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt as _, util::SubscriberInitExt as _};

use crate::{
    assets::Assets,
    handler::{handle_incoming, handle_outgoing},
    services::agent::{AgentRequest, AgentResponse, MessageRole, UiMessage},
    theme::change_color_mode,
    window::{blur_window, get_window_options},
};

actions!(window, [Quit, StandardAction]);

/// Available LLM models
pub const AVAILABLE_MODELS: &[(&str, &str)] = &[
    ("claude-haiku-4-5-20251001", "Claude Haiku 4.5"),
    ("claude-sonnet-4-5-20250929", "Claude Sonnet 4.5"),
    ("claude-opus-4-5-20251101", "Claude Opus 4.5"),
    ("claude-opus-4-1-20250805", "Claude Opus 4.1"),
];

pub struct MessageState {
    messages: Vec<UiMessage>,
}

pub struct ChatAI {
    text_input: Entity<InputState>,
    message_state: Entity<MessageState>,
    list_state: ListState,
    request_tx: Sender<AgentRequest>,
    model_select: Entity<SelectState<Vec<SharedString>>>,
    is_loading: bool,
}

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

impl ChatAI {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        /*
         * Create a channel
         * Spawn on the background and block there, send messages over the channel
         * Spawn on the foreground and listen to the channel
         * As events come in, send them over the channel to the main thread to be processed
         */
        let (response_tx, response_rx) = unbounded::<AgentResponse>();
        let (request_tx, request_rx) = unbounded::<AgentRequest>();

        // Spawn the agent message handler in backgrond
        cx.background_executor()
            .spawn(handle_outgoing(request_rx, response_tx))
            .detach();

        // Spawn foreground task to handle incoming responses from agent
        // detaching let's it run to execution
        cx.spawn(async move |this, cx| {
            handle_incoming(this, response_rx, cx).await;
        })
        .detach();

        let list_state = ListState::new(0, ListAlignment::Bottom, px(200.));

        // Initialize state with empty messages
        let message_state = cx.new(|_cx| MessageState { messages: vec![] });

        let model_names: Vec<SharedString> = AVAILABLE_MODELS
            .iter()
            .map(|(_, display_name)| SharedString::from(*display_name))
            .collect();

        // Default to first model
        let model_select =
            cx.new(|cx| SelectState::new(model_names, Some(IndexPath::new(0)), window, cx));

        // When messages are updated, update our list
        cx.observe(&message_state, |this: &mut ChatAI, _event, cx| {
            let items = this.message_state.read(cx).messages.clone();
            this.list_state = ListState::new(items.len(), ListAlignment::Bottom, px(20.));
            cx.notify();
        })
        .detach();

        // Subscribe to model selection changes
        let request_tx_for_select = request_tx.clone();
        cx.subscribe_in(
            &model_select,
            window,
            move |_this, _entity, event: &SelectEvent<Vec<SharedString>>, _window, _cx| {
                if let SelectEvent::Confirm(Some(selected_display_name)) = event {
                    // Find the model ID from the display name
                    if let Some((model_id, _)) = AVAILABLE_MODELS
                        .iter()
                        .find(|(_, display)| *display == selected_display_name.as_ref())
                    {
                        let _ = request_tx_for_select
                            .try_send(AgentRequest::SetModel(model_id.to_string()));
                    }
                }
            },
        )
        .detach();

        let text_input = cx.new(|cx| InputState::new(window, cx).placeholder("Ask me anything"));

        Self {
            text_input,
            message_state,
            list_state,
            request_tx,
            model_select,
            is_loading: false,
        }
    }
    pub fn add_message(&mut self, message: UiMessage, cx: &mut Context<Self>) {
        cx.update_entity(&self.message_state, |state, cx| {
            state.messages.push(message);
            cx.notify();
        });
    }

    pub fn set_loading(&mut self, loading: bool, cx: &mut Context<Self>) {
        self.is_loading = loading;
        cx.notify();
    }
    fn render_assistant(
        &mut self,
        ix: usize,
        item: UiMessage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let id: SharedString = format!("chat-{}", ix).into();
        div()
            .p_2()
            .child(TextView::markdown(id, item.content, window, cx).selectable(true))
    }

    fn render_user(
        &mut self,
        ix: usize,
        item: UiMessage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let id: SharedString = format!("chat-{}", ix).into();
        div()
            .p_2()
            .border_1()
            .bg(cx.theme().list_even)
            .border_color(cx.theme().border)
            .rounded_lg()
            .child(TextView::markdown(id, item.content, window, cx).selectable(true))
    }

    fn render_entry(
        &mut self,
        ix: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = self.message_state.read(cx).messages.clone();
        if items.len() == 0 {
            return div().into_any_element();
        }
        let item = items.get(ix).unwrap().clone();
        let elem = match item.role {
            MessageRole::ToolCall => div(),
            MessageRole::ToolResult => div(),
            MessageRole::Assistant => self.render_assistant(ix, item, window, cx),
            MessageRole::System => self.render_assistant(ix, item, window, cx),
            MessageRole::User => self.render_user(ix, item, window, cx),
        };

        div().p_1().child(elem).into_any_element()
    }
    fn on_send_message(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text_input.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        // Send chat request to agent
        let result = self.request_tx.try_send(AgentRequest::Chat(text.clone()));
        match result {
            Ok(_) => {
                tracing::debug!("Message sent successfully");
                // Add user message to display
                self.add_message(UiMessage::user(text), cx);
                self.set_loading(true, cx);
            }
            Err(e) => {
                tracing::error!("Failed to send message: {}", e);
                self.add_message(UiMessage::error(format!("Failed to send: {}", e)), cx);
            }
        }

        // Clear the textarea
        self.text_input.update(cx, |input, cx| {
            input.set_value("", window, cx);
        });

        cx.notify();
    }
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

impl Render for ChatAI {
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
                .child(Label::new("ChatAI"))
                .child(div().pr(px(5.0)).flex().items_center().child(theme_toggle)),
        );

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
                    .pl_2()
                    .items_center()
                    .child(Icon::empty().path("icons/anthropic.svg"))
                    .child(Select::new(&self.model_select).appearance(false)),
            )
            .child(
                Button::new("send")
                    .rounded_full()
                    .bg(cx.theme().accent)
                    .loading(self.is_loading.clone())
                    .icon(Icon::empty().path("icons/move-up.svg"))
                    .on_click(cx.listener(Self::on_send_message)),
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
                    .child(Input::new(&self.text_input.clone()).appearance(false)),
            )
            .child(form_footer);

        let items_len = self.message_state.read(cx).messages.clone().len();
        div().v_flex().size_full().child(header).child(
            div()
                .p_2()
                .v_flex()
                .size_full()
                .when(items_len == 0, |d| d.child(empty_content))
                .when(items_len > 0, |d| {
                    d.child(
                        div().p_2().size_full().flex().child(
                            list(
                                self.list_state.clone(),
                                cx.processor(|this, ix, window, cx| {
                                    this.render_entry(ix, window, cx)
                                }),
                            )
                            .size_full(),
                        ),
                    )
                })
                .child(form),
        )
    }
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

                let view = cx.new(|cx| ChatAI::new(window, cx));
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
