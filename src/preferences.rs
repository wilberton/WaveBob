//! Versioned layout preferences, independent of audio projects and undo history.
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread::JoinHandle;

use crate::presentation::{PanelLayout, PreferenceStatus};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    layout: PanelLayout,
}

pub(crate) struct Preferences {
    sender: Option<mpsc::Sender<(u64, PanelLayout)>>,
    results: mpsc::Receiver<(u64, io::Result<()>)>,
    worker: Option<JoinHandle<()>>,
    generation: u64,
}

impl Preferences {
    pub fn open(
        path: PathBuf,
        wake: impl Fn() + Send + 'static,
    ) -> (Self, PanelLayout, PreferenceStatus) {
        let (layout, status) = match load(&path) {
            Ok(layout) => (layout, PreferenceStatus::Saved),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (PanelLayout::default(), PreferenceStatus::Saved)
            }
            Err(error) => (
                PanelLayout::default(),
                PreferenceStatus::Error {
                    message: error.to_string(),
                },
            ),
        };
        let (sender, receiver) = mpsc::channel::<(u64, PanelLayout)>();
        let (done, results) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            while let Ok(mut pending) = receiver.recv() {
                for newer in receiver.try_iter() {
                    pending = newer;
                }
                let (generation, layout) = pending;
                let result = save(&path, layout);
                if done.send((generation, result)).is_err() {
                    break;
                }
                wake();
            }
        });
        (
            Self {
                sender: Some(sender),
                results,
                worker: Some(worker),
                generation: 0,
            },
            layout,
            status,
        )
    }

    pub fn save(&mut self, layout: &PanelLayout) {
        self.generation += 1;
        let _ = self
            .sender
            .as_ref()
            .unwrap()
            .send((self.generation, layout.clone()));
    }

    pub fn poll(&self) -> Option<PreferenceStatus> {
        self.results
            .try_iter()
            .filter(|(generation, _)| *generation == self.generation)
            .last()
            .map(|(_, result)| match result {
                Ok(()) => PreferenceStatus::Saved,
                Err(error) => PreferenceStatus::Error {
                    message: error.to_string(),
                },
            })
    }
}

impl Drop for Preferences {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn load(path: &Path) -> io::Result<PanelLayout> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(64 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 64 * 1024 {
        return Err(io::Error::other("Layout preferences exceed 64 KiB"));
    }
    let mut document: Document = serde_json::from_slice(&bytes)?;
    if !matches!(document.version, 1 | 2) {
        return Err(io::Error::other("Unsupported layout preference version"));
    }
    document
        .layout
        .validate()
        .map_err(|error| io::Error::other(error.message))?;
    if document.version == 1 {
        // Version 2 compacts the effect compositions. Apply this only while
        // loading old preferences, never to live geometry commands/user resizes.
        for geometry in document.layout.editor_geometry.values_mut() {
            geometry.width = (geometry.width * 0.75).max(320.0);
            geometry.height = (geometry.height * 0.75).max(240.0);
        }
    }
    Ok(document.layout)
}

fn save(path: &Path, layout: PanelLayout) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(temporary.as_file_mut(), &Document { version: 2, layout })?;
    temporary.as_file_mut().flush()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presentation::WindowGeometry;

    #[test]
    fn old_effect_window_sizes_compact_once_and_preserve_positions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("preferences.json");
        let mut layout = PanelLayout::default();
        layout.editor_geometry.insert(
            "old".into(),
            WindowGeometry {
                width: 850.0,
                height: 530.0,
                x: Some(-900.0),
                y: Some(120.0),
            },
        );
        std::fs::write(
            &path,
            serde_json::to_vec(&Document { version: 1, layout }).unwrap(),
        )
        .unwrap();
        let mut loaded = load(&path).unwrap();
        let geometry = &loaded.editor_geometry["old"];
        assert_eq!((geometry.width, geometry.height), (637.5, 397.5));
        assert_eq!((geometry.x, geometry.y), (Some(-900.0), Some(120.0)));
        save(&path, loaded.clone()).unwrap();
        assert_eq!(load(&path).unwrap(), loaded);
        loaded.editor_geometry.get_mut("old").unwrap().width = 900.0;
        save(&path, loaded.clone()).unwrap();
        assert_eq!(
            load(&path).unwrap(),
            loaded,
            "Subsequent user resizing must persist unchanged"
        );
    }
}
