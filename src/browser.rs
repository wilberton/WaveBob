//! Asynchronous filesystem navigation and cancellable, document-free audition.
use crate::{
    CommandError, ErrorCode, audio,
    documents::{DocumentInfo, SourceDetails, Wake},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
};

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct BrowserEntry {
    pub path: String,
    pub name: String,
    pub directory: bool,
    pub supported: bool,
}
#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct BrowserSnapshot {
    pub path: Option<String>,
    pub requested_path: Option<String>,
    pub parent: Option<String>,
    pub entries: Vec<BrowserEntry>,
    pub loading: bool,
    pub selected_path: Option<String>,
    pub skipped_paths: usize,
    pub error: Option<CommandError>,
    pub audition: AuditionSnapshot,
    pub audition_volume: f64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub struct AuditionSnapshot {
    pub id: Option<String>,
    pub path: Option<String>,
    pub loading: bool,
    pub playing: bool,
    pub frames: Option<u64>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u16>,
    pub error: Option<CommandError>,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PathParams {
    pub path: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AuditionVolumeParams {
    pub volume: f64,
}
enum Event {
    Listed(String, Result<Listing, CommandError>),
    Decoded(String, Result<audio::Audio, CommandError>),
}
struct Listing {
    path: String,
    parent: Option<String>,
    entries: Vec<BrowserEntry>,
    skipped: usize,
}
pub(crate) struct Browser {
    state: BrowserSnapshot,
    listing_id: Option<String>,
    cancel: Arc<AtomicBool>,
    sender: mpsc::Sender<Event>,
    receiver: mpsc::Receiver<Event>,
    audio: Option<Arc<audio::Audio>>,
    ready: bool,
    wake: Option<Wake>,
    listings: Arc<AtomicUsize>,
    decoders: Arc<AtomicUsize>,
}
impl Default for Browser {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            state: BrowserSnapshot {
                audition_volume: 0.75,
                ..Default::default()
            },
            listing_id: None,
            cancel: Arc::new(AtomicBool::new(false)),
            sender,
            receiver,
            audio: None,
            ready: false,
            wake: None,
            listings: Arc::new(AtomicUsize::new(0)),
            decoders: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl Browser {
    pub fn snapshot(&self) -> BrowserSnapshot {
        self.state.clone()
    }
    pub fn set_wake(&mut self, wake: Wake) {
        self.wake = Some(wake);
    }
    pub fn navigate(&mut self, path: String) -> Result<BrowserSnapshot, CommandError> {
        if path.trim().is_empty() {
            return Err(invalid("Choose a folder path"));
        }
        if self.listings.load(Ordering::Acquire) >= 2 {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Folder navigation is busy; retry shortly",
            ));
        }
        let listings = self.listings.clone();
        listings.fetch_add(1, Ordering::AcqRel);
        let id = uuid::Uuid::new_v4().to_string();
        self.listing_id = Some(id.clone());
        self.state.loading = true;
        self.state.requested_path = Some(path.clone());
        self.state.error = None;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let result = list(Path::new(&path));
            listings.fetch_sub(1, Ordering::Release);
            let _ = sender.send(Event::Listed(id, result));
            if let Some(wake) = wake {
                wake();
            }
        });
        Ok(self.snapshot())
    }
    pub fn select(&mut self, path: String) -> Result<BrowserSnapshot, CommandError> {
        if !self
            .state
            .entries
            .iter()
            .any(|entry| entry.path == path && !entry.directory)
        {
            return Err(invalid("Select a file in the current folder"));
        }
        self.state.selected_path = Some(path);
        Ok(self.snapshot())
    }
    pub fn start(&mut self, path: String) -> Result<BrowserSnapshot, CommandError> {
        if !supported(Path::new(&path)) {
            return Err(CommandError::new(
                ErrorCode::UnsupportedFormat,
                "Audition supports WAV and MP3 files",
            ));
        }
        if self.decoders.load(Ordering::Acquire) >= 2 {
            return Err(CommandError::new(
                ErrorCode::Busy,
                "Preview decoding is busy; retry shortly",
            ));
        }
        let decoders = self.decoders.clone();
        decoders.fetch_add(1, Ordering::AcqRel);
        self.stop();
        let id = uuid::Uuid::new_v4().to_string();
        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = cancel.clone();
        self.state.audition = AuditionSnapshot {
            id: Some(id.clone()),
            path: Some(path.clone()),
            loading: true,
            ..Default::default()
        };
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let result = audio::decode(&PathBuf::from(path), &cancel, |_| {});
            decoders.fetch_sub(1, Ordering::Release);
            if !cancel.load(Ordering::Acquire) {
                let _ = sender.send(Event::Decoded(id, result));
                if let Some(wake) = wake {
                    wake();
                }
            }
        });
        Ok(self.snapshot())
    }
    pub fn stop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.state.audition.id = None;
        self.state.audition.loading = false;
        self.state.audition.playing = false;
        self.audio = None;
        self.ready = false;
    }
    pub fn source_id(&self) -> Option<&str> {
        self.state.audition.id.as_deref()
    }
    pub fn set_playing(&mut self, playing: bool) {
        self.state.audition.playing = playing;
    }
    pub fn set_volume(&mut self, volume: f64) {
        self.state.audition_volume = volume;
    }
    pub fn fail(&mut self, error: CommandError) {
        self.state.audition.error = Some(error);
        self.stop();
    }
    pub fn take_ready(&mut self) -> Option<(DocumentInfo, Arc<audio::Audio>)> {
        if !std::mem::take(&mut self.ready) {
            return None;
        }
        let audio = self.audio.as_ref()?.clone();
        let id = self.state.audition.id.clone()?;
        Some((
            DocumentInfo {
                id: id.clone(),
                revision_id: id,
                path: self.state.audition.path.clone()?,
                name: "Browser audition".into(),
                sample_rate: audio.rate,
                channels: audio.channels,
                frames: audio.frames(),
                encoding: audio.encoding.clone(),
                source_details: SourceDetails::default(),
                dirty: false,
                cursor: 0,
                selection: None,
            },
            audio,
        ))
    }
    pub fn poll(&mut self) {
        for event in self.receiver.try_iter() {
            match event {
                Event::Listed(id, result) if self.listing_id.as_ref() == Some(&id) => {
                    self.state.loading = false;
                    match result {
                        Ok(listing) => {
                            self.state.path = Some(listing.path);
                            self.state.parent = listing.parent;
                            self.state.entries = listing.entries;
                            self.state.skipped_paths = listing.skipped;
                            self.state.selected_path = None;
                        }
                        Err(error) => self.state.error = Some(error),
                    }
                }
                Event::Decoded(id, result)
                    if self.state.audition.id.as_ref() == Some(&id)
                        && self.state.audition.loading =>
                {
                    self.state.audition.loading = false;
                    match result {
                        Ok(audio) => {
                            self.state.audition.frames = Some(audio.frames());
                            self.state.audition.sample_rate = Some(audio.rate);
                            self.state.audition.channels = Some(audio.channels);
                            self.audio = Some(Arc::new(audio));
                            self.ready = true;
                        }
                        Err(error) => self.state.audition.error = Some(error),
                    }
                }
                _ => {}
            }
        }
    }
}
impl Drop for Browser {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
    }
}
fn invalid(message: &str) -> CommandError {
    CommandError::new(ErrorCode::InvalidParams, message)
}
pub(crate) fn supported(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("wav") || s.eq_ignore_ascii_case("mp3"))
}
fn list(path: &Path) -> Result<Listing, CommandError> {
    let io = |error| {
        CommandError::new(
            ErrorCode::IoError,
            format!("Read folder {}: {error}", path.display()),
        )
    };
    let canonical = path.canonicalize().map_err(io)?;
    let entries = std::fs::read_dir(&canonical).map_err(io)?;
    let mut listing = Listing {
        path: canonical.to_string_lossy().into_owned(),
        parent: canonical.parent().map(|p| p.to_string_lossy().into_owned()),
        entries: Vec::new(),
        skipped: 0,
    };
    for entry in entries {
        if listing.entries.len() >= 10000 {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "Folder exceeds 10000 entries; choose a smaller folder",
            ));
        }
        let Ok(entry) = entry else {
            listing.skipped += 1;
            continue;
        };
        let path = entry.path();
        let Some(text) = path.to_str() else {
            listing.skipped += 1;
            continue;
        };
        let Ok(metadata) = std::fs::metadata(&path) else {
            listing.skipped += 1;
            continue;
        };
        if metadata.is_dir() || metadata.is_file() {
            listing.entries.push(BrowserEntry {
                path: text.into(),
                name: entry.file_name().to_string_lossy().into_owned(),
                directory: metadata.is_dir(),
                supported: metadata.is_file() && supported(&path),
            });
        }
    }
    listing
        .entries
        .sort_by_key(|entry| (!entry.directory, entry.name.to_lowercase()));
    Ok(listing)
}
