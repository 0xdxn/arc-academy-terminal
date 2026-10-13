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
    /// Jump to the newest output
    ScrollOutputEnd,
    /// Jump to the oldest output kept
    ScrollOutputStart,
    /// Show or hide the side panel
    ToggleSidebar,
    Enter,
    Escape,
    Help,
    CommandPalette,
    ToggleTheme,
    ToggleAI,
    ToggleSettings,
    ToggleLesson,
    /// Leave lesson mode (Ctrl+L from the lesson map, or the palette)
    ExitLessons,
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

        // Output scrolling from any panel. Ctrl+U/D work in every
        // terminal; macOS can claim Ctrl+arrows and the page keys.
        (KeyCode::Up, KeyModifiers::CONTROL) | (KeyCode::Up, KeyModifiers::SHIFT) => Action::ScrollOutputUp,
        (KeyCode::Down, KeyModifiers::CONTROL) | (KeyCode::Down, KeyModifiers::SHIFT) => Action::ScrollOutputDown,
        (KeyCode::End, _) => Action::ScrollOutputEnd,
        (KeyCode::Home, _) => Action::ScrollOutputStart,
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
        (KeyCode::Char('?'), KeyModifiers::NONE | KeyModifiers::SHIFT) | (KeyCode::F(1), _) => Action::Help,
        (KeyCode::Char('k'), KeyModifiers::CONTROL) => Action::CommandPalette,
        (KeyCode::Char('t'), KeyModifiers::CONTROL) => Action::ToggleTheme,
        (KeyCode::Char('a'), KeyModifiers::CONTROL) => Action::ToggleAI,
        (KeyCode::Char('s'), KeyModifiers::CONTROL) => Action::ToggleSettings,
        (KeyCode::Char('l'), KeyModifiers::CONTROL) => Action::ToggleLesson,
        (KeyCode::Char('m'), KeyModifiers::NONE) => Action::ShowLessonMenu,

        (KeyCode::Char('f'), KeyModifiers::CONTROL) => Action::ToggleSidebar,

        // Lesson step navigation. Ctrl keys work in every terminal; the
        // Alt keys only where Option/Alt is sent as Meta (on a Mac it
        // usually types a character instead)
        (KeyCode::Char('b'), KeyModifiers::CONTROL) => Action::LessonPreviousStep,
        (KeyCode::Left, KeyModifiers::ALT) => Action::LessonPreviousStep,
        (KeyCode::Char('r'), KeyModifiers::ALT) => Action::LessonRestart,

        // Gamification panels (also in the Ctrl+K palette)
        (KeyCode::Char('p'), KeyModifiers::CONTROL) => Action::ShowProgress,
        (KeyCode::Char('a'), KeyModifiers::ALT) => Action::ShowAchievements,
        (KeyCode::Char('p'), KeyModifiers::ALT) => Action::ShowProgress,
        (KeyCode::Char('c'), KeyModifiers::ALT) => Action::ShowChallenges,

        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(c: char) -> Action {
        key_to_action(KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL))
    }

    #[test]
    fn everyday_actions_have_ctrl_keys_that_work_in_any_terminal() {
        assert_eq!(ctrl('l'), Action::ToggleLesson);
        assert_eq!(ctrl('b'), Action::LessonPreviousStep);
        assert_eq!(ctrl('p'), Action::ShowProgress);
        assert_eq!(ctrl('f'), Action::ToggleSidebar);
        assert_eq!(ctrl('u'), Action::PageUp);
        assert_eq!(ctrl('d'), Action::PageDown);
        assert_eq!(ctrl('k'), Action::CommandPalette);
    }
}
