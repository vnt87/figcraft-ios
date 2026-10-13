//! The iPadOS platform services FigCraft's UI asks for: document pickers (UIKit's, answering on a
//! later frame) and the preferences kept in the app container.
use std::path::PathBuf;
use std::sync::mpsc;

use figcraft_engine::Prefs;
use figcraft_ui_egui::{PickRequest, Services};
use serde_json::{Value, json};

/// The preferences' file in the support folder.
const STATE: &str = "state.json";

/// The app's support folder, where its settings and managed documents live.
pub struct Platform {
    pub root: PathBuf,
}

fn support_directory() -> Result<PathBuf, String> {
    use objc2_foundation::{NSFileManager, NSSearchPathDirectory, NSSearchPathDomainMask};
    let directories = NSFileManager::defaultManager()
        .URLsForDirectory_inDomains(NSSearchPathDirectory::ApplicationSupportDirectory, NSSearchPathDomainMask::UserDomainMask);
    let path = directories.firstObject().and_then(|url| url.path()).ok_or("iOS Application Support directory is unavailable")?;
    let expected = crate::storage::app_support_dir(std::env::var_os("HOME"))?;
    let root = PathBuf::from(path.to_string()).join(expected.file_name().ok_or("Application Support name is missing")?);
    // The OS can spell the same container through /var or /private/var.
    std::fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    if root.canonicalize().map_err(|error| error.to_string())? != expected.canonicalize().map_err(|error| error.to_string())? {
        return Err("Application Support is outside the expected app container".into());
    }
    Ok(root)
}

/// Find the support folder and prepare the managed documents folder in it.
pub fn create() -> Result<Platform, String> {
    let root = support_directory()?;
    crate::documents::prepare(&root)?;
    Ok(Platform { root })
}

/// The saved preferences, if any.
pub fn load_prefs(root: &std::path::Path) -> Option<Prefs> {
    let state: Value = serde_json::from_str(&std::fs::read_to_string(root.join(STATE)).ok()?).ok()?;
    let mut prefs = serde_json::from_value::<Prefs>(state.get("prefs")?.clone()).ok()?;
    prefs.recent_files.truncate(figcraft_engine::cmd::file::MAX_RECENT);
    Some(prefs)
}

/// Write `prefs` atomically (a temporary file renamed over the old one: a crash leaves either version whole).
fn write_prefs(root: &std::path::Path, prefs: &Prefs) {
    let tmp = root.join("state.json.tmp");
    let written = std::fs::write(&tmp, json!({"prefs": prefs}).to_string()).and_then(|()| std::fs::rename(&tmp, root.join(STATE)));
    if let Err(error) = written {
        log::warn!("preferences: couldn't save them in {}: {error}", root.display());
    }
}

pub struct Ios {
    root: Option<PathBuf>,
    ui: egui::Context,
    answers: Option<mpsc::Receiver<Option<PathBuf>>>,
}

impl Ios {
    pub fn new(root: Option<PathBuf>, ui: egui::Context) -> Self {
        Self { root, ui, answers: None }
    }
}

impl Services for Ios {
    fn begin_pick(&mut self, request: &PickRequest) -> bool {
        let Some(root) = &self.root else { return false };
        let (tx, rx) = mpsc::channel();
        if !crate::documents::begin(request, root, crate::documents::Reply::new(tx, self.ui.clone())) {
            return false;
        }
        self.answers = Some(rx);
        true
    }

    fn poll_pick(&mut self) -> Option<Option<PathBuf>> {
        let answers = self.answers.as_ref()?;
        match answers.try_recv() {
            Ok(answer) => {
                self.answers = None;
                Some(answer)
            }
            Err(mpsc::TryRecvError::Empty) => None,
            // The picker went away without answering: that is a cancel.
            Err(mpsc::TryRecvError::Disconnected) => {
                self.answers = None;
                Some(None)
            }
        }
    }

    fn save_prefs(&mut self, prefs: &Prefs) {
        if let Some(root) = &self.root {
            write_prefs(root, prefs);
        }
    }
}

/// Keep the preferences now (the app is leaving the foreground or quitting).
pub fn save_state(root: &std::path::Path, prefs: &Prefs) {
    write_prefs(root, prefs);
}
