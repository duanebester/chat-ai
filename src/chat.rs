use crate::{
    handler::{handle_incoming, handle_outgoing},
    services::agent::{AgentRequest, AgentResponse, MessageRole, UiMessage},
    theme::change_color_mode,
};
use async_channel::{Sender, unbounded};
use gpui::{
    AnyElement, App, AppContext as _, ClickEvent, Context, Div, Entity, IntoElement, ListAlignment,
    ListState, ParentElement as _, PathPromptOptions, Render, SharedString, Styled as _, Window,
    div, list, prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IndexPath, Sizable as _, StyledExt as _, ThemeMode, TitleBar,
    alert::Alert,
    button::*,
    divider::Divider,
    h_flex,
    input::{Input, InputState},
    label::Label,
    select::{Select, SelectEvent, SelectState},
    text::TextView,
};
use std::{env, path::PathBuf};

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
    attached_files: Vec<PathBuf>,
    is_loading: bool,
    has_api_key: bool,
}

impl ChatAI {
    pub fn view(window: &mut Window, cx: &mut App) -> Entity<ChatAI> {
        cx.new(|cx| ChatAI::new(window, cx))
    }
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let has_api_key = env::var("ANTHROPIC_API_KEY").ok().is_some();

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

        let text_input = cx.new(|cx| {
            InputState::new(window, cx)
                .auto_grow(1, 5)
                .soft_wrap(true)
                .placeholder("Ask me anything")
        });

        Self {
            text_input,
            message_state,
            list_state,
            request_tx,
            model_select,
            is_loading: false,
            has_api_key,
            attached_files: vec![],
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

    fn on_submit(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.text_input.read(cx).text().to_string();
        if text.trim().is_empty() {
            return;
        }

        // Take attached files (clears them from state)
        let files = std::mem::take(&mut self.attached_files);

        // Send chat request to agent with files
        let result = self.request_tx.try_send(AgentRequest::Chat {
            content: text.clone(),
            files,
        });

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

    pub fn clear_chat(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.set_loading(true, cx);
        let result = self.request_tx.try_send(AgentRequest::ClearHistory);

        match result {
            Ok(_) => {
                tracing::debug!("Chat cleared successfully");
                cx.update_entity(&self.message_state, |state, cx| {
                    state.messages.clear();
                    cx.notify();
                });
            }
            Err(e) => {
                tracing::error!("Failed to clear chat: {}", e);
                self.add_message(UiMessage::error(format!("Failed to clear chat: {}", e)), cx);
            }
        }

        self.set_loading(false, cx);
        cx.notify();
    }

    fn attachment_label(&mut self) -> String {
        match self.attached_files.clone().len() {
            0 => "Attach file".to_string(),
            1 => "1 file".to_string(),
            n => format!("{} files", n),
        }
    }

    fn on_attach_file(&mut self, _: &ClickEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // Create the path prompt options - allow files, multiple selection
        let options = PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Select files to attach".into()),
        };

        // Get the receiver for the selected paths
        let paths_receiver = cx.prompt_for_paths(options);

        // Spawn an async task to handle the response
        cx.spawn(async move |this, cx| {
            // Wait for the user to select paths or cancel
            if let Ok(result) = paths_receiver.await {
                match result {
                    Ok(Some(paths)) => {
                        // User selected one or more paths
                        cx.update(|cx| {
                            let _ = this.update(cx, |chat, cx| {
                                for path in &paths {
                                    tracing::debug!("Attached file: {:?}", path);
                                }
                                chat.attached_files.extend(paths);
                                cx.notify();
                            });
                        })
                        .ok();
                    }
                    Ok(None) => {
                        // User cancelled the dialog
                        tracing::debug!("File selection cancelled");
                    }
                    Err(e) => {
                        tracing::error!("Error selecting files: {}", e);
                    }
                }
            }
        })
        .detach();
    }
}

impl Render for ChatAI {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let items_len = self.message_state.read(cx).messages.clone().len();

        let theme_toggle = Button::new("theme-mode")
            .map(|this| {
                if cx.theme().mode.is_dark() {
                    this.icon(Icon::empty().path("icons/sun.svg"))
                } else {
                    this.icon(Icon::empty().path("icons/moon.svg"))
                }
            })
            .small()
            .tooltip("Change mode")
            .ghost()
            .on_click(cx.listener(Self::change_mode));

        let clear_chat = Button::new("clear-chat")
            .icon(Icon::empty().path("icons/square-pen.svg"))
            .tooltip("New chat")
            .small()
            .ghost()
            .on_click(cx.listener(Self::clear_chat));

        let header = TitleBar::new().child(
            h_flex()
                .w_full()
                .py_1()
                .pr_1()
                .justify_between()
                .child(Label::new("ChatAI"))
                .child(
                    div()
                        .pr(px(5.0))
                        .flex()
                        .items_center()
                        .when(items_len > 0, |d| d.child(clear_chat))
                        .child(theme_toggle),
                ),
        );

        let empty_content = div()
            .flex()
            .flex_col()
            .flex_grow()
            .justify_end()
            .gap_4()
            .p_4()
            .when(!self.has_api_key.clone(), |d| {
                d.child(Alert::error("no-api-key", "No Anthropic API Key Found"))
            })
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
                Button::new("add-file")
                    .icon(Icon::empty().path("icons/paperclip.svg"))
                    .ghost()
                    .mr_1()
                    .on_click(cx.listener(Self::on_attach_file)),
            )
            .child(Divider::vertical())
            .child(Label::new(self.attachment_label()).pl_2());

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
                    .on_click(cx.listener(Self::on_submit)),
            );

        let form = div()
            .flex()
            .flex_col()
            .justify_between()
            .rounded_2xl()
            .border_1()
            .border_color(cx.theme().border.opacity(0.8))
            .bg(cx.theme().popover)
            .h(px(220.))
            .shadow_lg()
            .w_full()
            .child(
                div().flex().flex_col().child(form_header).child(
                    Input::new(&self.text_input.clone())
                        .appearance(false)
                        .disabled(!self.has_api_key.clone()),
                ),
            )
            .child(form_footer);

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
