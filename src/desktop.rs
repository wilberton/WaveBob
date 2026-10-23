//! Desktop presentation and render fences over the shared command interface.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use crate::{
    Command, CommandError, CommandResult, ErrorCode, Outcome, Request, Response, Session,
    SessionSnapshot, commands, ipc, presentation::*, protocol,
};
use eframe::egui;

enum RenderKind {
    Barrier,
    Capture {
        path: Option<String>,
        overwrite: bool,
    },
}

struct Pending {
    token: String,
    id: String,
    reply: mpsc::Sender<Response>,
    deadline: Instant,
    after_revision: u64,
    after_presentation_revision: u64,
    kind: RenderKind,
    in_flight: bool,
    cancelled: Arc<AtomicBool>,
}

pub struct DesktopHost {
    session: Session,
    presentation: PresentationSnapshot,
    server: Option<ipc::Server>,
    incoming: Option<mpsc::Receiver<ipc::Incoming>>,
    queue: VecDeque<ipc::Incoming>,
    pending: Option<Pending>,
    finished: mpsc::Receiver<String>,
    finish: mpsc::Sender<String>,
    directory: PathBuf,
    frame_number: u64,
    drawn_revision: u64,
    drawn_presentation_revision: u64,
    preferences: Option<crate::preferences::Preferences>,
}

impl Default for DesktopHost {
    fn default() -> Self {
        let (finish, finished) = mpsc::channel();
        Self {
            session: Session::default(),
            presentation: PresentationSnapshot::default(),
            server: None,
            incoming: None,
            queue: VecDeque::new(),
            pending: None,
            finish,
            finished,
            directory: PathBuf::new(),
            frame_number: 0,
            drawn_revision: 0,
            drawn_presentation_revision: 0,
            preferences: None,
        }
    }
}

impl DesktopHost {
    pub fn configure_preferences(&mut self, path: PathBuf, ctx: egui::Context) {
        let (preferences, layout, status) =
            crate::preferences::Preferences::open(path, move || {
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
                ctx.request_repaint();
            });
        self.preferences = Some(preferences);
        self.presentation.layout = layout;
        self.presentation.preferences = status;
    }

    pub fn listen(
        &mut self,
        directory: &Path,
        name: String,
        ctx: egui::Context,
    ) -> std::io::Result<()> {
        let (server, incoming) = ipc::Server::start(
            directory,
            &self.session.snapshot().session_id,
            name,
            move || {
                ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
                ctx.request_repaint();
            },
        )?;
        self.server = Some(server);
        self.incoming = Some(incoming);
        self.directory = directory.to_owned();
        Ok(())
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        self.session.snapshot()
    }
    pub fn presentation(&self) -> PresentationSnapshot {
        self.presentation.clone()
    }

    pub fn frozen(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.in_flight || self.ready(p))
    }

    pub fn execute(&mut self, request: Request) -> Response {
        if let Err(error) = self.session.validate(&request) {
            return self.session.failure(Some(request.id), error);
        }
        let expected = match &request.command {
            Command::SetMode(params) => params.expected_presentation_revision,
            Command::SetLayout(params) => params.expected_presentation_revision,
            Command::TogglePanels(params) => params.expected_presentation_revision,
            _ => None,
        };
        if expected.is_some_and(|revision| revision != self.presentation.revision) {
            return self.session.failure(
                Some(request.id),
                CommandError::new(ErrorCode::StaleRevision, "Presentation revision changed"),
            );
        }
        let mut presentation = self.presentation.clone();
        let result = match &request.command {
            Command::GetCapabilities(_) => {
                CommandResult::Capabilities(commands::capabilities(true))
            }
            Command::GetPresentation(_) => CommandResult::Presentation(self.presentation()),
            Command::SetMode(params) => {
                presentation.mode = params.mode;
                return self.commit_presentation(request.id, presentation, false);
            }
            Command::SetLayout(params) => {
                presentation.layout = match params.apply(&presentation.layout) {
                    Ok(layout) => layout,
                    Err(error) => return self.session.failure(Some(request.id), error),
                };
                return self.commit_presentation(request.id, presentation, true);
            }
            Command::TogglePanels(_) => {
                let show = !presentation.layout.left.visible && !presentation.layout.right.visible;
                presentation.layout.left.visible = show;
                presentation.layout.right.visible = show;
                return self.commit_presentation(request.id, presentation, true);
            }
            _ => return self.session.execute(request),
        };
        self.session.success(Some(request.id), result)
    }

    fn commit_presentation(
        &mut self,
        id: String,
        mut next: PresentationSnapshot,
        layout_requested: bool,
    ) -> Response {
        let layout_changed = next.layout != self.presentation.layout;
        if layout_changed || next.mode != self.presentation.mode {
            let Some(revision) = self.presentation.revision.checked_add(1) else {
                return self.session.failure(
                    Some(id),
                    CommandError::new(
                        ErrorCode::RevisionExhausted,
                        "Presentation revision exhausted",
                    ),
                );
            };
            next.revision = revision;
        }
        if (layout_changed
            || (layout_requested && matches!(next.preferences, PreferenceStatus::Error { .. })))
            && let Some(preferences) = &mut self.preferences
        {
            preferences.save(&next.layout);
            next.preferences = PreferenceStatus::Saving;
        }
        self.presentation = next;
        self.session
            .success(Some(id), CommandResult::Presentation(self.presentation()))
    }

    fn ready(&self, pending: &Pending) -> bool {
        self.session.snapshot().revision >= pending.after_revision
            && self.presentation.revision >= pending.after_presentation_revision
    }

    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        if let Some(status) = self
            .preferences
            .as_ref()
            .and_then(|preferences| preferences.poll())
        {
            self.presentation.preferences = status;
        }
        self.frame_number += 1;
        for token in self.finished.try_iter() {
            if self
                .pending
                .as_ref()
                .is_some_and(|pending| pending.token == token)
            {
                self.pending = None;
            }
        }
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| Instant::now() >= pending.deadline)
        {
            let pending = self.pending.take().unwrap();
            pending.cancelled.store(true, Ordering::Release);
            let _ = pending
                .reply
                .send(self.session.failure(Some(pending.id), timeout()));
        }
        if let Some(incoming) = &self.incoming {
            self.queue.extend(incoming.try_iter());
        }
        let count = self.queue.len();
        for _ in 0..count {
            if self.frozen() {
                break;
            }
            let incoming = self.queue.pop_front().unwrap();
            if Instant::now() >= incoming.deadline {
                let id = serde_json::from_str::<serde_json::Value>(&incoming.line)
                    .ok()
                    .and_then(|v| v.get("id")?.as_str().map(str::to_owned));
                let _ = incoming.reply.send(self.session.failure(id, timeout()));
                continue;
            }
            let request = match protocol::decode_line(&incoming.line) {
                Ok(request) => request,
                Err((id, error)) => {
                    let _ = incoming.reply.send(self.session.failure(id, error));
                    continue;
                }
            };
            if let Err(error) = self.session.validate(&request) {
                let _ = incoming
                    .reply
                    .send(self.session.failure(Some(request.id), error));
                continue;
            }
            let render = match &request.command {
                Command::RenderBarrier(params) => Some((
                    RenderKind::Barrier,
                    params.after_revision,
                    params.after_presentation_revision,
                    params.timeout_ms,
                )),
                Command::CapturePng(params) => Some((
                    RenderKind::Capture {
                        path: params.path.clone(),
                        overwrite: params.overwrite,
                    },
                    params.after_revision,
                    params.after_presentation_revision,
                    params.timeout_ms,
                )),
                _ => None,
            };
            if let Some((kind, after_revision, after_presentation_revision, timeout_ms)) = render {
                if !(1..=60_000).contains(&timeout_ms) {
                    let _ = incoming.reply.send(self.session.failure(
                        Some(request.id),
                        CommandError::new(
                            ErrorCode::InvalidParams,
                            "timeout_ms must be between 1 and 60000",
                        ),
                    ));
                } else if self.pending.is_some() {
                    self.queue.push_back(incoming);
                } else {
                    self.pending = Some(Pending {
                        token: uuid::Uuid::new_v4().to_string(),
                        id: request.id,
                        reply: incoming.reply,
                        deadline: incoming
                            .deadline
                            .min(Instant::now() + Duration::from_millis(timeout_ms)),
                        after_revision: after_revision.unwrap_or(self.session.snapshot().revision),
                        after_presentation_revision: after_presentation_revision
                            .unwrap_or(self.presentation.revision),
                        kind,
                        in_flight: false,
                        cancelled: Arc::new(AtomicBool::new(false)),
                    });
                }
            } else {
                let _ = incoming.reply.send(self.execute(request));
            }
        }
        if self.pending.is_some() || !self.queue.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint_after(Duration::from_millis(10));
        }
        self.drawn_revision = self.session.snapshot().revision;
        self.drawn_presentation_revision = self.presentation.revision;
    }

    /// Call after drawing the entire client area. Keep this frame's state fixed
    /// until the framebuffer readback callback has received its actual pixels.
    pub fn end_frame(&mut self, ctx: &egui::Context) {
        if self.drawn_revision != self.session.snapshot().revision
            || self.drawn_presentation_revision != self.presentation.revision
        {
            ctx.request_repaint();
            return;
        }
        let ready = self
            .pending
            .as_ref()
            .is_some_and(|p| !p.in_flight && self.ready(p));
        if !ready {
            return;
        }
        let pending = self.pending.as_mut().unwrap();
        pending.in_flight = true;
        let metadata = RenderMetadata {
            session_id: self.session.snapshot().session_id,
            rendered_revision: self.session.snapshot().revision,
            presentation_revision: self.presentation.revision,
            mode: self.presentation.mode,
            frame_number: self.frame_number,
            width: 0,
            height: 0,
            scale: ctx.pixels_per_point(),
            pending_resources: Vec::new(),
        };
        let response = self.session.success(
            Some(pending.id.clone()),
            CommandResult::Rendered(metadata.clone()),
        );
        let (reply, deadline, cancelled) = (
            pending.reply.clone(),
            pending.deadline,
            pending.cancelled.clone(),
        );
        let token = pending.token.clone();
        let destination = match &pending.kind {
            RenderKind::Barrier => None,
            RenderKind::Capture { path, overwrite } => Some((
                path.as_ref().map(PathBuf::from).unwrap_or_else(|| {
                    self.directory
                        .join("captures")
                        .join(format!("{}.png", uuid::Uuid::new_v4()))
                }),
                *overwrite,
                path.is_none(),
            )),
        };
        let (finish, ctx) = (self.finish.clone(), ctx.clone());
        ctx.clone().request_screenshot(move |image| {
            let _ = finish.send(token);
            ctx.request_repaint();
            std::thread::spawn(move || {
                if cancelled.load(Ordering::Acquire) {
                    return;
                }
                let mut metadata = metadata;
                metadata.width = image.size[0] as u32;
                metadata.height = image.size[1] as u32;
                let result = if Instant::now() >= deadline {
                    Err(timeout())
                } else if image.size.contains(&0) {
                    Err(CommandError::new(
                        ErrorCode::RenderFailed,
                        "Renderer returned an empty image",
                    ))
                } else if let Some((path, overwrite, create_parent)) = destination {
                    save_png(&image, path, overwrite, create_parent, deadline, &cancelled).map(
                        |path| {
                            CommandResult::Capture(CaptureResult {
                                path,
                                mime_type: "image/png".into(),
                                metadata,
                            })
                        },
                    )
                } else {
                    Ok(CommandResult::Rendered(metadata))
                };
                let mut response = response;
                response.outcome = match result {
                    Ok(result) => Outcome::Ok { result },
                    Err(error) => Outcome::Error { error },
                };
                let _ = reply.send(response);
            });
        });
    }
}

fn timeout() -> CommandError {
    CommandError::new(
        ErrorCode::Timeout,
        "Requested GUI revisions were not captured before the deadline",
    )
}

fn save_png(
    image: &egui::ColorImage,
    path: PathBuf,
    overwrite: bool,
    create_parent: bool,
    deadline: Instant,
    cancelled: &AtomicBool,
) -> Result<String, CommandError> {
    let io_error = |error: std::io::Error| CommandError::new(ErrorCode::IoError, error.to_string());
    let path = if path.is_absolute() {
        path
    } else {
        std::env::current_dir().map_err(io_error)?.join(path)
    };
    let parent = path.parent().ok_or_else(|| {
        CommandError::new(ErrorCode::InvalidParams, "Expected a PNG file destination")
    })?;
    if create_parent {
        std::fs::create_dir_all(parent).map_err(io_error)?;
    }
    let parent = parent.canonicalize().map_err(|error| {
        CommandError::new(
            ErrorCode::IoError,
            format!("Resolve capture directory: {error}"),
        )
    })?;
    let path = parent.join(
        path.file_name()
            .ok_or_else(|| CommandError::new(ErrorCode::InvalidParams, "Expected a file name"))?,
    );
    let path_string = path
        .to_str()
        .ok_or_else(|| {
            CommandError::new(
                ErrorCode::IoError,
                "Output path cannot be represented as UTF-8",
            )
        })?
        .to_owned();
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| {
        CommandError::new(
            ErrorCode::IoError,
            format!("Create capture temporary file: {error}"),
        )
    })?;
    let mut encoder = png::Encoder::new(
        file.as_file_mut(),
        image.size[0] as u32,
        image.size[1] as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let encode_error =
        |error: png::EncodingError| CommandError::new(ErrorCode::RenderFailed, error.to_string());
    let mut writer = encoder.write_header().map_err(encode_error)?;
    let pixels: Vec<u8> = image
        .pixels
        .iter()
        .flat_map(|pixel| pixel.to_srgba_unmultiplied())
        .collect();
    writer.write_image_data(&pixels).map_err(encode_error)?;
    writer.finish().map_err(encode_error)?;
    if Instant::now() >= deadline || cancelled.load(Ordering::Acquire) {
        return Err(timeout());
    }
    let result = if overwrite {
        file.persist(&path)
    } else {
        file.persist_noclobber(&path)
    };
    result.map_err(|error| {
        CommandError::new(
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                ErrorCode::OutputExists
            } else {
                ErrorCode::IoError
            },
            format!("Publish capture: {}", error.error),
        )
    })?;
    Ok(path_string)
}

impl Drop for DesktopHost {
    fn drop(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancelled.store(true, Ordering::Release);
            let _ = pending.reply.send(self.session.failure(
                Some(pending.id),
                CommandError::new(ErrorCode::RenderFailed, "GUI closed before capture"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_output_is_atomic_and_requires_explicit_overwrite() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("capture.png");
        let image = egui::ColorImage::filled([2, 2], egui::Color32::RED);
        let cancelled = AtomicBool::new(false);
        let write = |overwrite| {
            save_png(
                &image,
                path.clone(),
                overwrite,
                false,
                Instant::now() + Duration::from_secs(2),
                &cancelled,
            )
        };
        let result = write(false);
        assert!(result.is_ok(), "PNG publication failed: {result:?}");
        let original = std::fs::read(&path).unwrap();
        assert_eq!(write(false).unwrap_err().code, ErrorCode::OutputExists);
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert!(write(true).is_ok());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
