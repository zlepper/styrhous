//! Asks the user where to save a file, without blocking the UI thread.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// A dialog that closes faster than this cannot have been answered by a person,
/// so the platform could not show one (no portal or fallback on Linux).
const UNAVAILABLE_DIALOG_THRESHOLD: Duration = Duration::from_millis(300);

pub(crate) const SAVE_DIALOG_UNAVAILABLE: &str = "No save dialog could be opened. On Linux this needs xdg-desktop-portal with a file chooser backend.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SaveFileRequest {
    pub(crate) window_id: u64,
    pub(crate) export_id: u64,
    pub(crate) suggested_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SaveFileChoice {
    Chosen(PathBuf),
    Cancelled,
    /// The dialog could not be shown; the message explains why.
    Unavailable(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SaveFileAnswer {
    pub(crate) window_id: u64,
    pub(crate) export_id: u64,
    pub(crate) choice: SaveFileChoice,
}

/// The platform's save dialog. Answers arrive later through [`Self::poll`].
pub(crate) trait SaveFileDialog {
    fn request(&mut self, ctx: &egui::Context, request: SaveFileRequest);
    fn poll(&mut self) -> Vec<SaveFileAnswer>;
}

/// The native dialog (the desktop portal on Linux), shown from its own thread.
pub(crate) struct SystemSaveFileDialog {
    answers: mpsc::Receiver<SaveFileAnswer>,
    sender: mpsc::Sender<SaveFileAnswer>,
}

impl Default for SystemSaveFileDialog {
    fn default() -> Self {
        let (sender, answers) = mpsc::channel();
        Self { answers, sender }
    }
}

impl SaveFileDialog for SystemSaveFileDialog {
    fn request(&mut self, ctx: &egui::Context, request: SaveFileRequest) {
        let sender = self.sender.clone();
        let (window_id, export_id) = (request.window_id, request.export_id);
        let answer = move |choice| SaveFileAnswer {
            window_id,
            export_id,
            choice,
        };
        let failure_answer = answer(SaveFileChoice::Unavailable(
            "Could not open the save dialog".to_owned(),
        ));
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new()
            .name("save-file-dialog".to_owned())
            .spawn(move || {
                let opened = Instant::now();
                let path = rfd::FileDialog::new()
                    .set_file_name(&request.suggested_name)
                    .add_filter("Log files", &["log", "txt"])
                    .save_file();
                let choice = match path {
                    Some(path) => SaveFileChoice::Chosen(path),
                    None if opened.elapsed() < UNAVAILABLE_DIALOG_THRESHOLD => {
                        SaveFileChoice::Unavailable(SAVE_DIALOG_UNAVAILABLE.to_owned())
                    }
                    None => SaveFileChoice::Cancelled,
                };
                let _ = sender.send(answer(choice));
                ctx.request_repaint();
            });
        if spawned.is_err() {
            let _ = self.sender.send(failure_answer);
        }
    }

    fn poll(&mut self) -> Vec<SaveFileAnswer> {
        self.answers.try_iter().collect()
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Answers every request with a preset choice, recording the requests.
    #[derive(Clone)]
    pub(crate) struct MockSaveFileDialog {
        choice: SaveFileChoice,
        pub(crate) requests: Arc<Mutex<Vec<SaveFileRequest>>>,
        answers: Vec<SaveFileAnswer>,
    }

    impl MockSaveFileDialog {
        pub(crate) fn answering(choice: SaveFileChoice) -> Self {
            Self {
                choice,
                requests: Arc::default(),
                answers: Vec::new(),
            }
        }

        pub(crate) fn requests(&self) -> Vec<SaveFileRequest> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl SaveFileDialog for MockSaveFileDialog {
        fn request(&mut self, _ctx: &egui::Context, request: SaveFileRequest) {
            self.answers.push(SaveFileAnswer {
                window_id: request.window_id,
                export_id: request.export_id,
                choice: self.choice.clone(),
            });
            self.requests.lock().unwrap().push(request);
        }

        fn poll(&mut self) -> Vec<SaveFileAnswer> {
            std::mem::take(&mut self.answers)
        }
    }
}
