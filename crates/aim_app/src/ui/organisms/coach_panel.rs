//! The Coach page: opt-in switch, provider setup, the preview of what is sent, the chat,
//! generated scenario cards and the memory wipe.
//!
//! The panel only draws [`CoachState`] and returns a [`CoachAction`]; the menu page runs
//! each action as a background [`Job`] (`crate::persistence::coach`), so no request, key
//! lookup or file write ever blocks the frame.

use std::path::PathBuf;

use aim_ai::config::{LM_STUDIO_BASE_URL, OLLAMA_BASE_URL, OPENAI_BASE_URL, OPENCODE_BASE_URL};
use aim_ai::{CoachConfig, PromptInput, ProviderKind, cli};
use aim_core::coach::{CoachContext, Message, Turn, TurnEnd};
use aim_core::scenario::ScenarioDef;
use bevy_egui::egui;

use crate::persistence::coach::{Job, KeyStatus, MAX_STEPS, Phase};
use crate::ui::atoms::{ERROR, GOOD, WARN};

/// Canned prompt of "Analyse my stats".
pub const ANALYSE_PROMPT: &str = "Analyse my stats: call list_stats, then name my two \
     biggest weaknesses with the metric and threshold behind each, and one drill for each.";
/// Canned prompt of "Recommend a routine".
pub const ROUTINE_PROMPT: &str = "Recommend a routine for today with recommend_routine and \
     explain why it fits my weakest skills.";

/// What the player asked for on the Coach page.
#[derive(Debug, Clone, PartialEq)]
pub enum CoachAction {
    /// Send a chat message.
    Send(String),
    /// Stop waiting for the current answer.
    Cancel,
    /// Send a tiny prompt to check the provider settings.
    TestConnection,
    /// Store the typed key in the OS keyring.
    StoreKey,
    /// Remove the stored key from the OS keyring.
    ForgetKey,
    /// Rebuild the stats summary now.
    RefreshContext,
    /// Delete every note in the coach memory (already confirmed).
    WipeMemory,
    /// Check generated scenario `i` again (rules and playability).
    Check(usize),
    /// Play generated scenario `i`.
    Play(usize),
    /// Save generated scenario `i` to the custom scenarios folder.
    Save(usize),
    /// Start a new conversation.
    ClearChat,
}

/// A scenario the coach generated, waiting for the player's approval.
#[derive(Debug, Clone, PartialEq)]
pub struct ScenarioCard {
    pub def: ScenarioDef,
    /// The last check: the simulated contender's score, or every problem.
    pub check: Option<Result<f32, Vec<String>>>,
    /// Where it was saved, or why saving failed.
    pub saved: Option<Result<PathBuf, String>>,
}

/// The outcome of a card's background check or save.
#[derive(Debug, Clone, PartialEq)]
pub enum CardOutcome {
    Checked(usize, Result<f32, Vec<String>>),
    Saved(usize, Result<PathBuf, String>),
}

/// Everything the Coach page keeps between frames. Requests live in [`Job`]s.
#[derive(Default)]
pub struct CoachState {
    /// The chat message being typed.
    pub input: String,
    /// A key being typed; cleared as soon as it is handed to the keyring job.
    pub key_input: String,
    /// The conversation, as the core's messages.
    pub history: Vec<Message>,
    /// The last error or note for the player.
    pub notice: Option<String>,
    pub turn: Job<Turn>,
    pub test: Job<String>,
    pub test_result: Option<Result<String, String>>,
    pub key: Job<KeyStatus>,
    pub key_status: Option<KeyStatus>,
    /// The keyring account and environment variable `key_status` was checked for.
    pub key_checked_for: Option<(String, Option<String>)>,
    pub context: Job<CoachContext>,
    /// The cached summary the tools read and the preview shows.
    pub context_value: Option<CoachContext>,
    /// Identifies the history `context_value` was built from.
    pub context_key: Option<(usize, u64)>,
    pub cards: Vec<ScenarioCard>,
    pub card_job: Job<CardOutcome>,
    pub wipe: Job<()>,
    pub wipe_result: Option<Result<(), String>>,
    /// "Forget everything" was clicked once and waits for confirmation.
    pub confirm_wipe: bool,
}

impl CoachState {
    /// Whether any request is running (the page then repaints to animate the spinner).
    pub fn busy(&self) -> bool {
        self.turn.is_pending()
            || self.test.is_pending()
            || self.key.is_pending()
            || self.context.is_pending()
            || self.card_job.is_pending()
            || self.wipe.is_pending()
    }

    /// Stop every request that would send something (the coach was switched off).
    pub fn cancel_sending(&mut self) {
        self.turn.cancel();
        self.test.cancel();
    }

    /// Collect finished requests. Never blocks.
    pub fn poll(&mut self) {
        self.turn.poll();
        match self.turn.take() {
            Some(Ok(turn)) => self.apply_turn(turn),
            Some(Err(e)) => self.notice = Some(e),
            None => {}
        }
        self.test.poll();
        if let Some(result) = self.test.take() {
            self.test_result = Some(result);
        }
        self.key.poll();
        match self.key.take() {
            Some(Ok(status)) => self.key_status = Some(status),
            Some(Err(e)) => self.notice = Some(format!("OS keyring: {e}")),
            None => {}
        }
        self.context.poll();
        match self.context.take() {
            Some(Ok(ctx)) => self.context_value = Some(ctx),
            Some(Err(e)) => self.notice = Some(e),
            None => {}
        }
        self.card_job.poll();
        match self.card_job.take() {
            Some(Ok(CardOutcome::Checked(i, r))) => {
                if let Some(card) = self.cards.get_mut(i) {
                    card.check = Some(r);
                }
            }
            Some(Ok(CardOutcome::Saved(i, r))) => {
                if let Some(card) = self.cards.get_mut(i) {
                    card.saved = Some(r);
                }
            }
            Some(Err(e)) => self.notice = Some(e),
            None => {}
        }
        self.wipe.poll();
        if let Some(result) = self.wipe.take() {
            self.wipe_result = Some(result);
        }
    }

    /// Add a finished turn: its messages to the history, its scenarios as cards.
    pub fn apply_turn(&mut self, turn: Turn) {
        self.history.extend(turn.messages);
        self.cards
            .extend(turn.scenarios.into_iter().map(|def| ScenarioCard {
                def,
                check: None,
                saved: None,
            }));
        self.notice = match turn.end {
            TurnEnd::Answer(_) => None,
            TurnEnd::StepLimit => Some(format!(
                "The coach was still calling tools after {MAX_STEPS} steps and stopped. Ask \
                 again, more specifically."
            )),
        };
    }
}

/// One line of the conversation as the page shows it.
#[derive(Debug, Clone, PartialEq)]
pub enum Line {
    Player(String),
    Coach(String),
    /// A summary of the tools the coach used for one step.
    Tools(String),
}

/// The conversation without the system prompt, each step's tool calls summarised in one
/// line (with the failures), and the coach's text.
pub fn transcript(history: &[Message]) -> Vec<Line> {
    let mut lines = Vec::new();
    let mut i = 0;
    while i < history.len() {
        match &history[i] {
            Message::System { .. } | Message::Tool { .. } => {}
            Message::User { content } => lines.push(Line::Player(content.clone())),
            Message::Assistant {
                content,
                tool_calls,
            } => {
                if !content.trim().is_empty() {
                    lines.push(Line::Coach(content.clone()));
                }
                if !tool_calls.is_empty() {
                    let names: Vec<&str> = tool_calls.iter().map(|c| c.name.as_str()).collect();
                    let failed: Vec<&str> = history[i + 1..]
                        .iter()
                        .map_while(|m| match m {
                            Message::Tool { name, is_error, .. } => Some((name, *is_error)),
                            _ => None,
                        })
                        .filter(|(_, is_error)| *is_error)
                        .map(|(name, _)| name.as_str())
                        .collect();
                    let mut summary = format!("Used {}", names.join(", "));
                    if !failed.is_empty() {
                        summary.push_str(&format!(" ({} returned an error)", failed.join(", ")));
                    }
                    lines.push(Line::Tools(summary));
                }
            }
        }
        i += 1;
    }
    lines
}

/// Base URL presets of the OpenAI-compatible provider.
const BASE_URLS: [(&str, &str); 4] = [
    ("OpenAI", OPENAI_BASE_URL),
    ("OpenCode (pay-as-you-go)", OPENCODE_BASE_URL),
    ("Ollama", OLLAMA_BASE_URL),
    ("LM Studio", LM_STUDIO_BASE_URL),
];

const PROVIDERS: [(ProviderKind, &str); 3] = [
    (ProviderKind::OpenaiCompatible, "OpenAI-compatible"),
    (ProviderKind::Anthropic, "Anthropic"),
    (ProviderKind::CliDelegate, "CLI delegate"),
];

/// The Coach page. `memory_file` is shown next to the wipe button.
pub fn coach_panel(
    ui: &mut egui::Ui,
    state: &mut CoachState,
    config: &mut CoachConfig,
    memory_file: &str,
) -> Option<CoachAction> {
    let mut action = None;
    ui.heading("AI coach");
    ui.checkbox(
        &mut config.enabled,
        "Enable the AI coach (sends your stats summary and the chat to the provider you pick)",
    );
    ui.small(
        "Off by default. While it is off, nothing is sent. Only aggregates are sent: no raw \
         traces, file paths or names.",
    );
    ui.separator();

    egui::CollapsingHeader::new("Provider")
        .default_open(config.validate().is_err())
        .show(ui, |ui| {
            action = provider_section(ui, state, config).or(action.take());
        });

    egui::CollapsingHeader::new("Preview what is sent")
        .default_open(false)
        .show(ui, |ui| {
            action = preview_section(ui, state, config).or(action.take());
        });
    ui.separator();

    action = chat_section(ui, state, config).or(action.take());

    if !state.cards.is_empty() {
        ui.separator();
        action = cards_section(ui, state).or(action.take());
    }

    ui.separator();
    action = memory_section(ui, state, memory_file).or(action.take());
    action
}

fn provider_section(
    ui: &mut egui::Ui,
    state: &mut CoachState,
    config: &mut CoachConfig,
) -> Option<CoachAction> {
    let mut action = None;
    egui::Grid::new("coach_provider")
        .num_columns(2)
        .spacing([12.0, 6.0])
        .show(ui, |ui| {
            ui.label("Provider");
            ui.horizontal(|ui| {
                for (kind, label) in PROVIDERS {
                    ui.selectable_value(&mut config.provider, kind, label);
                }
            });
            ui.end_row();

            match config.provider {
                ProviderKind::OpenaiCompatible | ProviderKind::Anthropic => {
                    ui.label("Base URL");
                    ui.vertical(|ui| {
                        let hint = if config.provider == ProviderKind::Anthropic {
                            "default: https://api.anthropic.com"
                        } else {
                            "https://…/v1"
                        };
                        optional_text(ui, &mut config.base_url, hint);
                        if config.provider == ProviderKind::OpenaiCompatible {
                            ui.horizontal_wrapped(|ui| {
                                for (label, url) in BASE_URLS {
                                    if ui.small_button(label).clicked() {
                                        config.base_url = Some(url.to_owned());
                                    }
                                }
                            });
                        }
                    });
                    ui.end_row();

                    ui.label("Model");
                    ui.add(
                        egui::TextEdit::singleline(&mut config.model)
                            .hint_text("model id, e.g. llama3.1 or claude-sonnet-5-5")
                            .desired_width(280.0),
                    );
                    ui.end_row();

                    ui.label("API key");
                    ui.vertical(|ui| {
                        action = key_rows(ui, state).or(action.take());
                    });
                    ui.end_row();

                    ui.label("Key env var");
                    optional_text(
                        ui,
                        &mut config.api_key_env,
                        "optional fallback, e.g. OPENAI_API_KEY",
                    );
                    ui.end_row();
                }
                ProviderKind::CliDelegate => {
                    ui.label("Command");
                    ui.vertical(|ui| {
                        if config.cli_command.is_empty() {
                            ui.colored_label(WARN, "Pick your installed CLI:");
                        } else {
                            ui.monospace(config.cli_command.join(" "));
                        }
                        ui.horizontal(|ui| {
                            let presets = [
                                ("Codex", cli::codex_command(), PromptInput::Stdin),
                                ("Claude Code", cli::claude_command(), PromptInput::Stdin),
                                ("OpenCode", cli::opencode_command(), PromptInput::Argument),
                            ];
                            for (label, command, input) in presets {
                                if ui.small_button(label).clicked() {
                                    config.cli_command = command;
                                    config.cli_prompt = input;
                                }
                            }
                        });
                        ui.small(
                            "Codex calls the coach's tools through aimtrainer-mcp, so it sees \
                             your stats summary. Claude Code and OpenCode are text only. Each \
                             uses its own sign-in.",
                        );
                    });
                    ui.end_row();
                }
            }

            ui.label("Timeout");
            ui.add(
                egui::DragValue::new(&mut config.request_timeout_secs)
                    .range(1..=aim_ai::config::MAX_REQUEST_TIMEOUT_SECS)
                    .suffix(" s"),
            );
            ui.end_row();
        });

    if let Err(errors) = config.validate() {
        for e in errors {
            ui.colored_label(ERROR, e);
        }
    }

    ui.horizontal(|ui| {
        let can_test = config.enabled && config.validate().is_ok() && !state.test.is_pending();
        if ui
            .add_enabled(can_test, egui::Button::new("Test connection"))
            .on_disabled_hover_text("Enable the coach and fix the settings first.")
            .clicked()
        {
            action = Some(CoachAction::TestConnection);
        }
        if state.test.is_pending() {
            ui.spinner();
            ui.label("Testing…");
        } else {
            match &state.test_result {
                Some(Ok(reply)) => {
                    ui.colored_label(GOOD, format!("Connected. Reply: {}", one_line(reply, 80)));
                }
                Some(Err(e)) => {
                    ui.colored_label(ERROR, e);
                }
                None => {}
            }
        }
    });
    action
}

/// The key status and the field that stores a new key. The key is never shown again.
fn key_rows(ui: &mut egui::Ui, state: &mut CoachState) -> Option<CoachAction> {
    let mut action = None;
    match &state.key_status {
        _ if state.key.is_pending() => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Checking the OS keyring…");
            });
        }
        Some(KeyStatus::Keyring) => {
            ui.colored_label(GOOD, "Key stored in OS keyring");
        }
        Some(KeyStatus::Env(var)) => {
            ui.colored_label(GOOD, format!("Key from env {var}"));
        }
        Some(KeyStatus::Missing) => {
            ui.colored_label(WARN, "No key stored (local servers need none)");
        }
        Some(KeyStatus::Unavailable(e)) => {
            ui.colored_label(ERROR, format!("OS keyring unavailable: {e}"));
        }
        Some(KeyStatus::NotNeeded) | None => {}
    }
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut state.key_input)
                .password(true)
                .hint_text("paste a key to store it")
                .desired_width(220.0),
        );
        let ready = !state.key_input.trim().is_empty() && !state.key.is_pending();
        if ui
            .add_enabled(ready, egui::Button::new("Store in keyring"))
            .clicked()
        {
            action = Some(CoachAction::StoreKey);
        }
        if state.key_status == Some(KeyStatus::Keyring)
            && ui
                .add_enabled(!state.key.is_pending(), egui::Button::new("Forget key"))
                .clicked()
        {
            action = Some(CoachAction::ForgetKey);
        }
    });
    action
}

fn preview_section(
    ui: &mut egui::Ui,
    state: &CoachState,
    config: &CoachConfig,
) -> Option<CoachAction> {
    let mut action = None;
    ui.small(if config.provider == ProviderKind::CliDelegate {
        "The CLI delegate never receives this: it sees only the chat."
    } else {
        "This exact JSON is what the model receives when it calls list_stats, plus the chat \
         and the coach's instructions."
    });
    ui.horizontal(|ui| {
        if ui
            .add_enabled(!state.context.is_pending(), egui::Button::new("Refresh"))
            .clicked()
        {
            action = Some(CoachAction::RefreshContext);
        }
        if state.context.is_pending() {
            ui.spinner();
        }
    });
    match &state.context_value {
        Some(ctx) => {
            let json = ctx.to_prompt_json();
            ui.small(format!("{} bytes", json.len()));
            egui::ScrollArea::vertical()
                .id_salt("coach_preview")
                .max_height(220.0)
                .show(ui, |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(json).monospace().small()).wrap());
                });
        }
        None => {
            ui.label("Preparing your stats summary…");
        }
    }
    action
}

fn chat_section(
    ui: &mut egui::Ui,
    state: &mut CoachState,
    config: &CoachConfig,
) -> Option<CoachAction> {
    let mut action = None;
    ui.heading("Chat");
    let ready = config.enabled
        && config.validate().is_ok()
        && state.context_value.is_some()
        && !state.turn.is_pending();
    let why_not = if !config.enabled {
        "Enable the coach first."
    } else if config.validate().is_err() {
        "Fix the provider settings first."
    } else if state.context_value.is_none() {
        "Your stats summary is still being prepared."
    } else {
        "Wait for the current answer."
    };
    ui.horizontal(|ui| {
        for (label, prompt) in [
            ("Analyse my stats", ANALYSE_PROMPT),
            ("Recommend a routine", ROUTINE_PROMPT),
        ] {
            if ui
                .add_enabled(ready, egui::Button::new(label))
                .on_disabled_hover_text(why_not)
                .clicked()
            {
                action = Some(CoachAction::Send(prompt.to_owned()));
            }
        }
        if !state.history.is_empty()
            && ui
                .add_enabled(!state.turn.is_pending(), egui::Button::new("New chat"))
                .clicked()
        {
            action = Some(CoachAction::ClearChat);
        }
    });

    egui::ScrollArea::vertical()
        .id_salt("coach_chat")
        .max_height(320.0)
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for line in transcript(&state.history) {
                match line {
                    Line::Player(text) => {
                        ui.label(egui::RichText::new(format!("You: {text}")).strong());
                    }
                    Line::Coach(text) => {
                        ui.label(format!("Coach: {text}"));
                    }
                    Line::Tools(text) => {
                        ui.small(text);
                    }
                }
                ui.add_space(4.0);
            }
        });

    ui.horizontal(|ui| {
        let response = ui.add(
            egui::TextEdit::singleline(&mut state.input)
                .hint_text("Ask the coach…")
                .desired_width(360.0),
        );
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let can_send = ready && !state.input.trim().is_empty();
        let clicked = ui
            .add_enabled(can_send, egui::Button::new("Send"))
            .on_disabled_hover_text(why_not)
            .clicked();
        if (clicked || enter) && can_send {
            action = Some(CoachAction::Send(state.input.trim().to_owned()));
        }
        if state.turn.is_pending() {
            ui.spinner();
            ui.label("The coach is thinking…");
            if ui.button("Cancel").clicked() {
                action = Some(CoachAction::Cancel);
            }
        } else if let Phase::Cancelled = state.turn.request().phase() {
            ui.small("Cancelled.");
        }
    });
    if let Some(notice) = &state.notice {
        ui.colored_label(ERROR, notice);
    }
    action
}

fn cards_section(ui: &mut egui::Ui, state: &CoachState) -> Option<CoachAction> {
    let mut action = None;
    ui.heading("Generated scenarios");
    ui.small("Uncalibrated: the coach's numbers are estimates. Check, play, then save the ones you like.");
    let busy = state.card_job.is_pending();
    for (i, card) in state.cards.iter().enumerate() {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.strong(&card.def.name);
            ui.small(format!("{} · {}", card.def.id, card.def.description));
            match &card.check {
                Some(Ok(score)) => {
                    ui.colored_label(
                        GOOD,
                        format!("Valid and playable (simulated contender scored {score:.0})"),
                    );
                }
                Some(Err(errors)) => {
                    for e in errors {
                        ui.colored_label(ERROR, e);
                    }
                }
                None => {}
            }
            match &card.saved {
                Some(Ok(path)) => {
                    ui.colored_label(
                        GOOD,
                        format!(
                            "Saved to {} (listed under Scenarios after a restart)",
                            path.display()
                        ),
                    );
                }
                Some(Err(e)) => {
                    ui.colored_label(ERROR, e);
                }
                None => {}
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new("Validate"))
                    .clicked()
                {
                    action = Some(CoachAction::Check(i));
                }
                if ui.button("Play").clicked() {
                    action = Some(CoachAction::Play(i));
                }
                let saved = matches!(card.saved, Some(Ok(_)));
                if ui
                    .add_enabled(!busy && !saved, egui::Button::new("Save"))
                    .clicked()
                {
                    action = Some(CoachAction::Save(i));
                }
            });
        });
    }
    action
}

fn memory_section(
    ui: &mut egui::Ui,
    state: &mut CoachState,
    memory_file: &str,
) -> Option<CoachAction> {
    let mut action = None;
    ui.heading("Memory");
    ui.small(format!(
        "What the coach remembers about your goals and habits stays on this computer: {memory_file}"
    ));
    ui.horizontal(|ui| {
        if state.wipe.is_pending() {
            ui.spinner();
            ui.label("Forgetting…");
        } else if state.confirm_wipe {
            ui.label("Delete every note the coach kept?");
            if ui.button("Forget everything").clicked() {
                action = Some(CoachAction::WipeMemory);
                state.confirm_wipe = false;
            }
            if ui.button("Cancel").clicked() {
                state.confirm_wipe = false;
            }
        } else {
            if ui.button("Forget everything").clicked() {
                state.confirm_wipe = true;
                state.wipe_result = None;
            }
            match &state.wipe_result {
                Some(Ok(())) => {
                    ui.colored_label(GOOD, "The coach's memory is empty.");
                }
                Some(Err(e)) => {
                    ui.colored_label(ERROR, e);
                }
                None => {}
            }
        }
    });
    action
}

/// A text field over an optional setting: empty means `None`.
fn optional_text(ui: &mut egui::Ui, value: &mut Option<String>, hint: &str) {
    let mut text = value.clone().unwrap_or_default();
    if ui
        .add(
            egui::TextEdit::singleline(&mut text)
                .hint_text(hint)
                .desired_width(280.0),
        )
        .changed()
    {
        *value = (!text.is_empty()).then_some(text);
    }
}

/// `text` on one line, cut to `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match line.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aim_core::coach::ToolCall;
    use serde_json::json;

    fn assistant(text: &str, tools: &[&str]) -> Message {
        Message::Assistant {
            content: text.into(),
            tool_calls: tools
                .iter()
                .enumerate()
                .map(|(i, name)| ToolCall {
                    id: format!("c{i}"),
                    name: (*name).into(),
                    arguments: json!({}),
                })
                .collect(),
        }
    }

    fn tool(name: &str, is_error: bool) -> Message {
        Message::Tool {
            call_id: "c".into(),
            name: name.into(),
            content: "{}".into(),
            is_error,
        }
    }

    #[test]
    fn the_transcript_hides_the_system_prompt_and_summarises_tool_activity() {
        let history = vec![
            Message::System {
                content: "instructions".into(),
            },
            Message::User {
                content: "How am I?".into(),
            },
            assistant("", &["list_stats", "create_scenario"]),
            tool("list_stats", false),
            tool("create_scenario", true),
            assistant("Train micro-adjustments.", &[]),
        ];
        assert_eq!(
            transcript(&history),
            vec![
                Line::Player("How am I?".into()),
                Line::Tools(
                    "Used list_stats, create_scenario (create_scenario returned an error)".into()
                ),
                Line::Coach("Train micro-adjustments.".into()),
            ]
        );
    }

    #[test]
    fn a_turn_adds_its_messages_and_scenario_cards_and_a_step_limit_is_explained() {
        let mut state = CoachState::default();
        let def =
            aim_core::coach::drills::generate(aim_core::coach::SubSkill::MicroAdjustment, 0.0, 1)
                .unwrap();
        state.apply_turn(Turn {
            messages: vec![
                Message::User {
                    content: "Make a drill".into(),
                },
                assistant("Here.", &[]),
            ],
            end: TurnEnd::Answer("Here.".into()),
            scenarios: vec![def.clone()],
        });
        assert_eq!(state.history.len(), 2);
        assert_eq!(state.cards.len(), 1);
        assert_eq!(state.cards[0].def, def);
        assert_eq!(state.notice, None);

        state.apply_turn(Turn {
            messages: vec![],
            end: TurnEnd::StepLimit,
            scenarios: vec![],
        });
        assert!(state.notice.as_deref().is_some_and(|n| n.contains("steps")));
    }

    #[test]
    fn long_replies_are_cut_on_one_line() {
        assert_eq!(one_line("a\nb  c", 10), "a b c");
        assert_eq!(one_line("abcdef", 3), "abc…");
    }
}
