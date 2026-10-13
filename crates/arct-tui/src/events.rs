//! Event handling for the TUI

use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers, MouseEvent};
use std::time::Duration;
use tokio::sync::mpsc;

/// Events that can occur in the application
#[derive(Debug, Clone)]
pub enum Event {
    /// Terminal key press
    Key(KeyEvent),

    /// Mouse click / scroll (mouse capture is enabled)
    Mouse(MouseEvent),

    /// Terminal resize
    Resize(u16, u16),

    /// Application tick (for animations/updates)
    Tick,

    /// Request to quit
    Quit,
}

/// Key actions for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    NextPanel,
    PreviousPanel,
    ScrollUp,
    ScrollDown,
    PageUp,
    PageDown,
    ScrollOutputUp,    // Always scrolls output regardless of focus
    ScrollOutputDown,  // Always scrolls output regardless of focus
    Enter,
    Escape,
    Help,
    CommandPalette,
    ToggleTheme,
    ToggleAI,
    ToggleSettings,
    ToggleLesson,
    ShowLessonMenu,
    LessonPreviousStep,
    LessonRestart,
    /// Switch lesson practice between the simulated sandbox and the real
    /// filesystem playground (~/ArcAcademy/playground)
    TogglePracticeMode,
    /// Wipe and re-materialize the current lesson's playground directory
    ResetPlayground,
    ShowAchievements,
    ShowProgress,
    ShowChallenges,
    /// Open First Keys, the full-screen mode for pre-readers
    StartFirstKeys,
    DismissNotification,
    None,
}

/// Event handler that polls for terminal events
pub struct EventHandler {
    /// Sender handed off to the polling task on start; kept as an Option so
    /// the channel closes (and `next()` returns None) if the polling task
    /// ever stops
    sender: Option<mpsc::UnboundedSender<Event>>,
    receiver: mpsc::UnboundedReceiver<Event>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();
        Self {
            sender: Some(sender),
            receiver,
        }
    }

    /// Start the event polling loop
    pub async fn start(&mut self) {
        let Some(sender) = self.sender.take() else {
            // Already started
            return;
        };

        tokio::spawn(async move {
            /// Give up (closing the channel so the main loop exits) after
            /// this many consecutive terminal I/O failures
            const MAX_CONSECUTIVE_ERRORS: u32 = 10;

            let mut consecutive_errors: u32 = 0;

            loop {
                // Poll for events with a timeout
                match event::poll(Duration::from_millis(100)) {
                    Ok(true) => match event::read() {
                        Ok(CrosstermEvent::Key(key)) => {
                            consecutive_errors = 0;
                            if sender.send(Event::Key(key)).is_err() {
                                break;
                            }
                        }
                        Ok(CrosstermEvent::Mouse(mouse)) => {
                            consecutive_errors = 0;
                            if sender.send(Event::Mouse(mouse)).is_err() {
                                break;
                            }
                        }
                        Ok(CrosstermEvent::Resize(width, height)) => {
                            consecutive_errors = 0;
                            if sender.send(Event::Resize(width, height)).is_err() {
                                break;
                            }
                        }
                        Ok(_) => {
                            consecutive_errors = 0;
                        }
                        Err(e) => {
                            consecutive_errors += 1;
                            tracing::warn!(
                                "Failed to read terminal event ({}/{}): {}",
                                consecutive_errors,
                                MAX_CONSECUTIVE_ERRORS,
                                e
                            );
                            if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                                tracing::error!("Too many terminal read failures, stopping event loop");
                                break;
                            }
                        }
                    },
                    Ok(false) => {
                        consecutive_errors = 0;
                        // Send tick event
                        if sender.send(Event::Tick).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        consecutive_errors += 1;
                        tracing::warn!(
                            "Failed to poll terminal events ({}/{}): {}",
                            consecutive_errors,
                            MAX_CONSECUTIVE_ERRORS,
                            e
                        );
                        if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                            tracing::error!("Too many terminal poll failures, stopping event loop");
                            break;
                        }
                    }
                }
            }
        });
    }

    /// Receive the next event
    pub async fn next(&mut self) -> Option<Event> {
        self.receiver.recv().await
    }
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert key events to actions
pub fn key_to_action(key: KeyEvent) -> Action {
    match (key.code, key.modifiers) {
        // Quit
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,
        (KeyCode::Char('q'), KeyModifiers::NONE) => Action::Quit,
        (KeyCode::Esc, _) => Action::Escape,

        // Navigation
        (KeyCode::Tab, KeyModifiers::NONE) => Action::NextPanel,
        (KeyCode::BackTab, KeyModifiers::SHIFT) => Action::PreviousPanel,

        // Output scrolling (works from any panel - Ctrl+Arrow or Ctrl+J/K)
        (KeyCode::Up, KeyModifiers::CONTROL) => Action::ScrollOutputUp,
        (KeyCode::Down, KeyModifiers::CONTROL) => Action::ScrollOutputDown,
        (KeyCode::Char('j'), KeyModifiers::CONTROL) => Action::ScrollOutputDown,
        // Note: Ctrl+K is CommandPalette, so use Alt+J/K as alternative
        (KeyCode::Char('j'), KeyModifiers::ALT) => Action::ScrollOutputDown,
        (KeyCode::Char('k'), KeyModifiers::ALT) => Action::ScrollOutputUp,

        // Panel-specific scrolling (only when Output panel focused)
        (KeyCode::Up, KeyModifiers::NONE) | (KeyCode::Char('k'), KeyModifiers::NONE) => Action::ScrollUp,
        (KeyCode::Down, KeyModifiers::NONE) | (KeyCode::Char('j'), KeyModifiers::NONE) => Action::ScrollDown,
        (KeyCode::PageUp, _) | (KeyCode::Char('u'), KeyModifiers::CONTROL) => Action::PageUp,
        (KeyCode::PageDown, _) | (KeyCode::Char('d'), KeyModifiers::CONTROL) => Action::PageDown,

        // Actions
        (KeyCode::Enter, _) => Action::Enter,
        (KeyCode::Char('?'), KeyModifiers::NONE) => Action::Help,
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Action::CommandPalette,
        (KeyCode::Char('t'), KeyModifiers::CONTROL) => Action::ToggleTheme,
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => Action::ToggleAI,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::ToggleSettings,
        (KeyCode::Char('l'), KeyModifiers::CONTROL) => Action::ToggleLesson,
        (KeyCode::Char('m'), KeyModifiers::NONE) => Action::ShowLessonMenu,

        // Lesson step navigation (Alt modifier so plain typing is unaffected)
        (KeyCode::Left, KeyModifiers::ALT) => Action::LessonPreviousStep,
        (KeyCode::Char('r'), KeyModifiers::ALT) => Action::LessonRestart,

        // Gamification panels (Alt modifier to avoid conflicts with typing)
        (KeyCode::Char('a'), KeyModifiers::ALT) => Action::ShowAchievements,
        (KeyCode::Char('p'), KeyModifiers::ALT) => Action::ShowProgress,
        (KeyCode::Char('c'), KeyModifiers::ALT) => Action::ShowChallenges,

        _ => Action::None,
    }
}
