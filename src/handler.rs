use async_channel::{Receiver, Sender};
use gpui::{AppContext, AsyncApp, WeakEntity};

use crate::{
    ChatAI,
    services::agent::{
        Agent, AgentRequest, AgentResponse, ContentBlock, FileSource, UiMessage, upload_file,
    },
};

pub async fn handle_outgoing(
    request_rx: Receiver<AgentRequest>,
    response_tx: Sender<AgentResponse>,
) {
    if let Some(mut agent) = Agent::builder()
        .system_prompt(
            "You are a helpful, succint assistant. Please respond only in markdown and no emojis."
                .to_string(),
        )
        .max_tokens(4096)
        .build(vec![])
        .ok()
    {
        // Get API key for file uploads
        let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();

        while let Ok(request) = request_rx.recv().await {
            match request {
                AgentRequest::Chat { content, files } => {
                    // Build user content with text and any uploaded files
                    let mut user_content = vec![ContentBlock::Text { text: content }];

                    // Upload files and add to content
                    for path in files {
                        match smol::unblock({
                            let api_key = api_key.clone();
                            let path = path.clone();
                            move || upload_file(&api_key, &path)
                        })
                        .await
                        {
                            Ok(file_id) => {
                                user_content.push(ContentBlock::Document {
                                    source: FileSource::File { file_id },
                                });
                            }
                            Err(e) => {
                                tracing::error!("Failed to upload file: {}", e);
                                let _ = response_tx.try_send(AgentResponse::Error(format!(
                                    "Failed to upload file: {}",
                                    e
                                )));
                            }
                        }
                    }

                    match agent.chat_step(user_content).await {
                        Ok(response) => {
                            let _ = response_tx.try_send(response);
                        }
                        Err(e) => {
                            let _ = response_tx.try_send(AgentResponse::Error(format!("{}", e)));
                        }
                    }
                }
                AgentRequest::ClearHistory => {
                    agent.clear_conversation();
                }
                AgentRequest::SetModel(model) => {
                    // Update the agent's model
                    tracing::debug!("Setting agent model to: {}", model);
                    agent.set_model(model);
                    // Clear conversation when model changes
                    agent.clear_conversation();
                }
                _ => {}
            }
        }
    } else {
        tracing::error!("Failed to build agent");
        let _ = response_tx.try_send(AgentResponse::Error(
            "Failed to initialize agent".to_string(),
        ));
    }
}

pub async fn handle_incoming(
    this: WeakEntity<ChatAI>,
    response_rx: Receiver<AgentResponse>,
    cx: &mut AsyncApp,
) {
    loop {
        let incoming_response = response_rx.recv().await;
        match incoming_response {
            Ok(response) => {
                // Check if this response means we're done processing
                let is_done = response.is_done();

                match response {
                    AgentResponse::TextResponse { text, .. } => {
                        if let Some(view) = this.upgrade() {
                            let _ = cx.update_entity(&view, |this, cx| {
                                this.add_message(UiMessage::assistant(text), cx);
                                // Clear loading state only if done
                                if is_done {
                                    this.set_loading(false, cx);
                                }
                            });
                        }
                    }
                    AgentResponse::Error(err) => {
                        if let Some(view) = this.upgrade() {
                            let _ = cx.update_entity(&view, |this, cx| {
                                this.add_message(UiMessage::error(err), cx);
                                // Always clear loading state on error
                                this.set_loading(false, cx);
                            });
                        }
                    }
                    _ => {}
                }
            }
            Err(e) => {
                tracing::error!("Channel error: {}", e);
                if let Some(view) = this.upgrade() {
                    let _ = cx.update_entity(&view, |this, cx| {
                        // TODO: notify of error
                        this.set_loading(false, cx);
                    });
                }
                break;
            }
        }
    }
}
