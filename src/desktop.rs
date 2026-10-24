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
mod editors;

enum RenderKind {
    Barrier,
    Capture {
        path: Option<String>,
        overwrite: bool,
    },
}

struct Pending {
    editor_id: Option<String>,
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
    transient_resources: Vec<String>,
    ui_resources: Vec<String>,
    editor_previews: std::collections::BTreeMap<String, String>,
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
            transient_resources: Vec::new(),
            ui_resources: Vec::new(),
            editor_previews: Default::default(),
        }
    }
}

impl DesktopHost {
    pub fn with_session(session: Session) -> Self {
        let mut host = Self::default();
        host.session = session;
        host
    }
    pub fn transport(&self) -> crate::transport::TransportSnapshot {
        self.session.transport_snapshot()
    }
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
        let job_context = ctx.clone();
        self.session.set_waker(Arc::new(move || {
            job_context.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            job_context.request_repaint();
        }));
        #[cfg(feature = "audio-output")]
        if !self.session.audio_output_configured() {
            self.session.enable_native_output();
        }
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
    pub fn browser(&self) -> crate::browser::BrowserSnapshot {
        self.session.browser_snapshot()
    }
    pub fn mix(&self) -> crate::mixing::MixState {
        self.session.mix_snapshot()
    }
    pub fn history(&self) -> crate::effects::HistoryInfo {
        self.session.history_snapshot()
    }
    pub fn clipboard(&self) -> crate::edits::ClipboardInfo {
        self.session.clipboard_snapshot()
    }
    pub fn initialize_browser(&mut self, path: String) {
        self.session.initialize_browser(path);
    }
    pub fn presentation(&self) -> PresentationSnapshot {
        self.presentation.clone()
    }
    pub fn set_transient_resources(&mut self, resources: Vec<String>) {
        self.transient_resources = resources;
    }
    pub fn set_ui_resources(&mut self, resources: Vec<String>) {
        self.ui_resources = resources;
    }
    pub fn ready_to_close(&self) -> bool {
        self.presentation.close_requested
            && self.server.as_ref().is_none_or(ipc::Server::is_idle)
            && self.queue.is_empty()
            && self.pending.is_none()
    }

    pub fn frozen(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.in_flight || self.ready(p))
    }

    pub fn execute(&mut self, request: Request) -> Response {
        if self.presentation.close_requested
            && request.command.mutates()
            && !matches!(&request.command, Command::RequestQuit(_))
        {
            return self.session.failure(
                Some(request.id),
                CommandError::new(
                    ErrorCode::Busy,
                    "This GUI is closing and no longer accepts mutations",
                ),
            );
        }
        if let Err(error) = self.session.validate(&request) {
            return self.session.failure(Some(request.id), error);
        }
        let expected = match &request.command {
            Command::OpenEffectEditor(params)
            | Command::FocusEffectEditor(params)
            | Command::CloseEffectEditor(params) => params.expected_presentation_revision,
            Command::SetEditorGeometry(params) => params.expected_presentation_revision,
            Command::SetMode(params) => params.expected_presentation_revision,
            Command::SetTimelineViewport(params) => params.expected_presentation_revision,
            Command::SetEffect(params) => params.expected_presentation_revision,
            Command::RequestQuit(params) => params.expected_presentation_revision,
            Command::SetViewport(params) => params.expected_presentation_revision,
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
        if matches!(
            &request.command,
            Command::OpenEffectEditor(_)
                | Command::FocusEffectEditor(_)
                | Command::CloseEffectEditor(_)
                | Command::SetEditorGeometry(_)
        ) {
            return self.editor_command(request);
        }
        let mut presentation = self.presentation.clone();
        let result = match &request.command {
            Command::GetCapabilities(_) => CommandResult::Capabilities(commands::capabilities(
                true,
                self.session.audio_output_configured(),
            )),
            Command::GetPresentation(_) => CommandResult::Presentation(self.presentation().into()),
            Command::RequestQuit(params) => {
                if let Err(error) = self.session.check_quit(params.discard_changes) {
                    return self.session.failure(Some(request.id), error);
                }
                presentation.close_requested = true;
                return self.commit_presentation(request.id, presentation, false);
            }
            Command::SetEffect(params) => {
                if let Err(error) = params.effect.validate() {
                    return self.session.failure(Some(request.id), error);
                }
                presentation.effect = params.effect.clone();
                return self.commit_presentation(request.id, presentation, false);
            }
            Command::SetTimelineViewport(params) => {
                if params.start_frame >= params.end_frame
                    || params.end_frame > crate::arrangement::MAX_TIMELINE_FRAME
                {
                    return self.session.failure(
                        Some(request.id),
                        CommandError::new(
                            ErrorCode::InvalidParams,
                            "Timeline viewport must be nonempty within 24 hours",
                        ),
                    );
                }
                presentation.timeline = crate::documents::FrameRange {
                    start: params.start_frame,
                    end: params.end_frame,
                };
                return self.commit_presentation(request.id, presentation, false);
            }
            Command::SetMode(params) => {
                if params.mode != presentation.mode {
                    if self.presentation.revision == u64::MAX {
                        return self.session.failure(
                            Some(request.id),
                            CommandError::new(
                                ErrorCode::RevisionExhausted,
                                "Presentation revision exhausted",
                            ),
                        );
                    }
                    if let Err(error) = self.session.pause_output() {
                        return self.session.failure(Some(request.id), error);
                    }
                }
                presentation.mode = params.mode;
                return self.commit_presentation(request.id, presentation, false);
            }
            Command::SetViewport(params) => {
                let snapshot = self.session.snapshot();
                let Some(document) = snapshot
                    .documents
                    .iter()
                    .find(|doc| doc.id == params.document_id)
                else {
                    return self.session.failure(
                        Some(request.id),
                        CommandError::new(ErrorCode::NotFound, "Unknown document identifier"),
                    );
                };
                if params.start_frame >= params.end_frame || params.end_frame > document.frames {
                    return self.session.failure(
                        Some(request.id),
                        CommandError::new(
                            ErrorCode::InvalidParams,
                            "Visible interval must contain at least one frame within the document",
                        ),
                    );
                }
                presentation.viewports.insert(
                    params.document_id.clone(),
                    crate::documents::FrameRange {
                        start: params.start_frame,
                        end: params.end_frame,
                    },
                );
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
            _ => return self.session.execute_committed(request),
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
        if layout_changed
            || next.mode != self.presentation.mode
            || next.viewports != self.presentation.viewports
            || next.editor_windows != self.presentation.editor_windows
            || next.effect != self.presentation.effect
            || next.close_requested != self.presentation.close_requested
        {
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
        self.session.success(
            Some(id),
            CommandResult::Presentation(self.presentation().into()),
        )
    }

    fn ready(&self, pending: &Pending) -> bool {
        self.session.snapshot().revision >= pending.after_revision
            && self.presentation.revision >= pending.after_presentation_revision
    }

    pub fn begin_frame(&mut self, ctx: &egui::Context) {
        if !self.frozen() {
            self.session.poll_jobs();
            self.sync_editors();
            let snapshot = self.session.snapshot();
            let mut next = self.presentation.clone();
            next.viewports.retain(|id, view| {
                if let Some(document) = snapshot.documents.iter().find(|doc| &doc.id == id) {
                    *view = self.presentation.visible_interval(document);
                    true
                } else {
                    false
                }
            });
            if next.viewports != self.presentation.viewports {
                self.commit_presentation("viewport-sync".into(), next, false);
            }
        }
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
                    params.editor_id.clone(),
                )),
                Command::CapturePng(params) => Some((
                    RenderKind::Capture {
                        path: params.path.clone(),
                        overwrite: params.overwrite,
                    },
                    params.after_revision,
                    params.after_presentation_revision,
                    params.timeout_ms,
                    params.editor_id.clone(),
                )),
                _ => None,
            };
            if let Some((
                kind,
                after_revision,
                after_presentation_revision,
                timeout_ms,
                editor_id,
            )) = render
            {
                if editor_id
                    .as_ref()
                    .is_some_and(|id| !self.presentation.editor_windows.iter().any(|w| &w.id == id))
                {
                    let _ = incoming.reply.send(self.session.failure(
                        Some(request.id),
                        CommandError::new(ErrorCode::NotFound, "Requested editor window is closed"),
                    ));
                } else if !(1..=60_000).contains(&timeout_ms) {
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
                        editor_id,
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
        if let Some(id) = self.pending.as_ref().and_then(|p| p.editor_id.as_ref()) {
            let viewport = egui::ViewportId::from_hash_of(id);
            ctx.send_viewport_cmd_to(viewport, egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint_of(viewport);
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
        self.end_viewport_frame(ctx, None);
    }
    pub fn editor_capture_requested(&self, id: &str) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|p| p.editor_id.as_deref() == Some(id))
    }
    pub fn end_editor_frame(&mut self, ctx: &egui::Context, id: &str) {
        self.end_viewport_frame(ctx, Some(id));
    }
    fn end_viewport_frame(&mut self, ctx: &egui::Context, editor_id: Option<&str>) {
        if self
            .pending
            .as_ref()
            .is_none_or(|p| p.editor_id.as_deref() != editor_id)
        {
            return;
        }
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
        let window = editor_id
            .and_then(|id| self.presentation.editor_windows.iter().find(|w| w.id == id))
            .cloned();
        let state = window
            .as_ref()
            .and_then(|window| self.processing(window.owner.clone()).ok());
        let processing_revision = state.as_ref().map(|s| s.committed.edit_revision_id.clone());
        let processing_identity = state.as_ref().map(|s| {
            crate::projects::checksum(&(s.preview.as_ref().unwrap_or(&s.committed), s.effect_tempo))
        });
        let preview_id = state.and_then(|s| s.preview_id);
        let pending = self.pending.as_mut().unwrap();
        pending.in_flight = true;
        let metadata = RenderMetadata {
            editor_id: editor_id.map(str::to_owned),
            owner: window.as_ref().map(|w| w.owner.clone()),
            instance_id: window.as_ref().map(|w| w.instance_id.clone()),
            processing_revision,
            processing_identity,
            preview_id,
            session_id: self.session.snapshot().session_id,
            rendered_revision: self.session.snapshot().revision,
            presentation_revision: self.presentation.revision,
            mode: self.presentation.mode,
            frame_number: self.frame_number,
            width: 0,
            height: 0,
            scale: ctx.pixels_per_point(),
            pending_resources: self
                .session
                .snapshot()
                .jobs
                .into_iter()
                .filter(|job| job.status == crate::documents::JobStatus::Running)
                .map(|job| format!("job:{}", job.id))
                .chain(self.transient_resources.iter().cloned())
                .chain(self.ui_resources.iter().cloned())
                .chain(self.session.transport_snapshot().pending.then(|| {
                    format!(
                        "transport-handoff:{}",
                        self.session.transport_snapshot().requested.generation
                    )
                }))
                .chain(
                    self.session
                        .browser_snapshot()
                        .loading
                        .then(|| "browser-listing".into()),
                )
                .chain(
                    self.session
                        .browser_snapshot()
                        .audition
                        .loading
                        .then(|| "browser-audition-decode".into()),
                )
                .collect(),
        };
        let response = self.session.success(
            Some(pending.id.clone()),
            CommandResult::Rendered(metadata.clone().into()),
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
                            CommandResult::Capture(Box::new(CaptureResult {
                                path,
                                mime_type: "image/png".into(),
                                metadata,
                            }))
                        },
                    )
                } else {
                    Ok(CommandResult::Rendered(metadata.into()))
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
    fn effect_capture_is_issued_only_on_its_requested_viewport() {
        let mut host = DesktopHost::default();
        let (reply, _receive) = mpsc::channel();
        let id = "capture-shell-probe".to_owned();
        host.presentation.editor_windows.push(EditorWindow {
            id: id.clone(),
            owner: crate::processing::Owner::Document(uuid::Uuid::new_v4().to_string()),
            instance_id: uuid::Uuid::new_v4().to_string(),
            focus_generation: 1,
        });
        host.pending = Some(Pending {
            editor_id: Some(id.clone()),
            token: "probe".into(),
            id: "capture".into(),
            reply,
            deadline: Instant::now() + Duration::from_secs(5),
            after_revision: 0,
            after_presentation_revision: 0,
            kind: RenderKind::Capture {
                path: None,
                overwrite: false,
            },
            in_flight: false,
            cancelled: Arc::new(AtomicBool::new(false)),
        });
        let ctx = egui::Context::default();
        host.end_frame(&ctx);
        host.end_editor_frame(&ctx, "another-window");
        assert!(!host.pending.as_ref().unwrap().in_flight);
        let viewport = egui::ViewportId::from_hash_of(&id);
        let mut input = egui::RawInput {
            viewport_id: viewport,
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::Vec2::new(720.0, 460.0),
            )),
            ..Default::default()
        };
        input.viewports.insert(
            viewport,
            egui::ViewportInfo {
                parent: Some(egui::ViewportId::ROOT),
                ..Default::default()
            },
        );
        let mut output = ctx.run_ui(input, |ui| host.end_editor_frame(ui.ctx(), &id));
        assert!(host.pending.as_ref().unwrap().in_flight);
        assert!(
            output.viewport_output[&viewport]
                .commands
                .iter()
                .any(|command| matches!(command, egui::ViewportCommand::Screenshot(_)))
        );
        assert!(
            !output
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|root| root
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::Screenshot(_))))
        );
        output.textures_delta.clear();
    }

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
