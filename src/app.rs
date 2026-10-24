use eframe::egui::{self, Align, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use std::{collections::HashSet, sync::mpsc};
use wavebob::{Command, EmptyParams, Outcome, Request, desktop::DesktopHost, presentation::*};
use wavebob::{CommandResult, documents::*};
#[path = "app/arranging.rs"]
mod arranging;
#[path = "app/browser.rs"]
mod browser;
#[path = "app/dropping.rs"]
mod dropping;
#[path = "app/editing.rs"]
mod editing;
#[path = "app/effect_controls.rs"]
mod effect_controls;
#[path = "app/effects.rs"]
mod effects;
#[path = "app/projecting.rs"]
mod projecting;
#[path = "app/saving.rs"]
mod saving;
#[path = "app/soft_clipper.rs"]
mod soft_clipper;
#[path = "app/visuals.rs"]
mod visuals;
#[path = "app/waveform.rs"]
mod waveform;
use visuals::{Icon, icon_button};

const BLUE: Color32 = Color32::from_rgb(0, 122, 255);
const SURFACE: Color32 = Color32::from_rgb(20, 26, 33);
const BORDER: Color32 = Color32::from_rgb(49, 60, 73);
const MUTED: Color32 = Color32::from_rgb(145, 160, 179);

#[derive(Default)]
pub struct WaveBob {
    host: DesktopHost,
    shared_self: std::sync::Weak<std::sync::Mutex<WaveBob>>,
    error: Option<String>,
    waveform: Option<WaveformData>,
    detail: Option<WaveformData>,
    gesture: Option<waveform::Gesture>,
    open_requested: bool,
    file_imports: dropping::FileImports,
    dialog: Option<mpsc::Receiver<Option<std::path::PathBuf>>>,
    reported_jobs: HashSet<String>,
    browser_path: String,
    browser_seen_path: Option<String>,
    browser_edit_path: bool,
    native_editors: std::collections::BTreeMap<String, effects::NativeEditor>,
    stack_drag: Option<String>,
    output_dialog: Option<saving::OutputDialog>,
    save_picker: Option<mpsc::Receiver<Option<std::path::PathBuf>>>,
    project_picker: Option<projecting::ProjectPicker>,
    close_flow: Option<saving::CloseFlow>,
    close_save_job: Option<String>,
    os_close_pending: bool,
    arrangement_gesture: Option<arranging::Gesture>,
    clip_waves: std::collections::BTreeMap<String, WaveformData>,
    track_drafts: std::collections::BTreeMap<String, arranging::TrackDraft>,
    clip_trim_draft: Option<arranging::ClipTrimDraft>,
    master_gain_draft: Option<(String, f64)>,
}

#[derive(Default)]
struct Help {
    hover: Option<String>,
    focus: Option<String>,
}

impl Help {
    fn observe(&mut self, response: &egui::Response, text: &str) {
        if response.contains_pointer()
            && response.ctx.input(|input| {
                input
                    .pointer
                    .hover_pos()
                    .is_some_and(|pos| response.interact_rect.contains(pos))
            })
            && self.hover.is_none()
        {
            self.hover = Some(text.into());
        }
        if response.has_focus() && self.focus.is_none() {
            self.focus = Some(text.into());
        }
    }
    fn text(&self) -> &str {
        self.hover
            .as_deref()
            .or(self.focus.as_deref())
            .unwrap_or("")
    }
}

impl WaveBob {
    fn open_path(&mut self, path: std::path::PathBuf) {
        if let Some(path) = path.to_str() {
            self.dispatch(Command::OpenDocument(OpenParams { path: path.into() }));
            self.set_mode(ViewMode::Waveform);
        } else {
            self.error = Some("The selected path is not valid Unicode.".into());
        }
    }
    fn poll_dialog(&mut self) {
        if self.host.frozen() {
            return;
        }
        if let Some(dialog) = &self.dialog {
            match dialog.try_recv() {
                Ok(path) => {
                    self.dialog = None;
                    if let Some(path) = path {
                        self.open_path(path);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.dialog = None;
                    self.error = Some("The file dialog failed to open.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
    fn start_dialog(&mut self, ctx: &egui::Context) {
        if !self.open_requested || self.dialog.is_some() {
            return;
        }
        self.open_requested = false;
        let (sender, receiver) = mpsc::channel();
        self.dialog = Some(receiver);
        let ctx = ctx.clone();
        // RFD dispatches windowed macOS dialogs to AppKit's main thread.
        std::thread::spawn(move || {
            let path = rfd::FileDialog::new()
                .set_title("Open audio")
                .add_filter("WAV and MP3 audio", &["wav", "mp3"])
                .pick_file();
            let _ = sender.send(path);
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        });
    }
    fn active_document(&self) -> Option<DocumentInfo> {
        let snapshot = self.host.snapshot();
        snapshot
            .documents
            .into_iter()
            .find(|doc| Some(&doc.id) == snapshot.active_document_id.as_ref())
    }
    fn read_waveform(
        &mut self,
        document: &DocumentInfo,
        start: u64,
        end: u64,
    ) -> Option<WaveformData> {
        let response = self.host.execute(Request::new(
            "gui-waveform",
            Command::GetWaveform(WaveformParams {
                document_id: document.id.clone(),
                revision_id: Some(document.revision_id.clone()),
                start_frame: start,
                end_frame: Some(end),
                bins: 2048,
            }),
        ));
        match response.outcome {
            Outcome::Ok {
                result: CommandResult::Waveform(wave),
            } => Some(wave),
            Outcome::Error { error } => {
                self.error = Some(error.message);
                None
            }
            _ => None,
        }
    }
    fn update_waveform(&mut self) {
        if let Some(document) = self.active_document() {
            if self.waveform.as_ref().is_none_or(|wave| {
                wave.document_id != document.id || wave.revision_id != document.revision_id
            }) {
                self.waveform = self.read_waveform(&document, 0, document.frames);
            }
            let visible = self.host.presentation().visible_interval(&document);
            if self.detail.as_ref().is_none_or(|wave| {
                wave.document_id != document.id
                    || wave.revision_id != document.revision_id
                    || wave.start_frame != visible.start
                    || wave.end_frame != visible.end
            }) {
                self.detail = if visible.start == 0 && visible.end == document.frames {
                    self.waveform.clone()
                } else {
                    self.read_waveform(&document, visible.start, visible.end)
                };
            }
        } else {
            self.waveform = None;
            self.detail = None;
        }
        for job in self.host.snapshot().jobs {
            if job.kind == "Prepare mix" {
                continue;
            }
            if job.status == JobStatus::Failed && self.reported_jobs.insert(job.id) {
                self.error = job.error.map(|error| error.message);
            }
        }
    }
    fn dispatch(&mut self, command: Command) {
        if self.host.frozen() {
            return;
        }
        if matches!(
            &command,
            Command::SetMode(_)
                | Command::SetLayout(_)
                | Command::TogglePanels(_)
                | Command::ActivateDocument(_)
                | Command::CloseDocument(_)
        ) {
            self.gesture = None;
        }
        let response = self.host.execute(Request::new("gui", command));
        self.error = match response.outcome {
            Outcome::Error { error } => Some(error.message),
            _ => None,
        };
    }
    fn toggle_loop(&mut self) {
        self.dispatch(Command::ToggleLoop(EmptyParams {}));
    }
    fn toggle_playback(&mut self) {
        if self.host.frozen() {
            return;
        }
        self.gesture = None;
        if self.host.presentation().mode == ViewMode::Waveform
            && let Some(document) = self.active_document()
        {
            self.dispatch(Command::TogglePlayback(DocumentParams {
                document_id: document.id,
            }));
        } else if self.host.presentation().mode == ViewMode::Multitrack {
            self.dispatch(Command::ToggleMixPlayback(EmptyParams {}));
        }
    }
    fn rewind(&mut self) {
        if self.host.frozen() {
            return;
        }
        self.gesture = None;
        if self.host.presentation().mode == ViewMode::Waveform
            && let Some(document) = self.active_document()
        {
            self.dispatch(Command::RewindDocument(DocumentParams {
                document_id: document.id,
            }));
        } else if self.host.presentation().mode == ViewMode::Multitrack {
            self.dispatch(Command::RewindMix(EmptyParams {}));
        }
    }
    fn set_mode(&mut self, mode: ViewMode) {
        self.dispatch(Command::SetMode(SetModeParams {
            mode,
            expected_presentation_revision: None,
        }));
    }
    fn layout(&mut self, params: SetLayoutParams) {
        self.dispatch(Command::SetLayout(params));
    }

    pub fn new(
        cc: &eframe::CreationContext<'_>,
        directory: &std::path::Path,
        name: String,
        preferences: std::path::PathBuf,
    ) -> std::io::Result<Self> {
        theme(&cc.egui_ctx);
        let mut app = Self::default();
        app.host
            .configure_preferences(preferences, cc.egui_ctx.clone());
        app.host.listen(directory, name, cc.egui_ctx.clone())?;
        if let Ok(path) = std::env::current_dir() {
            app.browser_path = path.to_string_lossy().into_owned();
            app.host.initialize_browser(app.browser_path.clone());
        }
        Ok(app)
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.host.frozen()
            || ctx.text_edit_focused()
            || self.dialog.is_some()
            || self.open_requested
            || self.output_dialog.is_some()
            || self.save_picker.is_some()
            || self.project_picker.is_some()
            || self.close_flow.is_some()
            || !ctx.input(|input| input.focused)
            || ctx.memory(|memory| memory.top_modal_layer().is_some())
            || egui::Popup::is_any_open(ctx)
        {
            return;
        }
        let pressed = |key| {
            ctx.input_mut(|input| {
            let fresh = input.events.iter().any(|event| matches!(event, egui::Event::Key { key: actual, pressed: true, repeat: false, modifiers, .. } if *actual == key && modifiers.is_none()));
            fresh && input.consume_key(egui::Modifiers::NONE, key)
        })
        };
        let select_all=ctx.input(|input|input.events.iter().any(|event|matches!(event,egui::Event::Key{key:egui::Key::A,pressed:true,repeat:false,modifiers,..} if (modifiers.command||modifiers.ctrl||modifiers.mac_cmd) && !modifiers.shift && !modifiers.alt)));
        let history_key = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if (modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
                    && !modifiers.alt =>
                {
                    match key {
                        egui::Key::Z => Some(modifiers.shift),
                        egui::Key::Y if !modifiers.shift => Some(true),
                        _ => None,
                    }
                }
                _ => None,
            })
        });
        if let Some(redo) = history_key {
            let history = self.host.history();
            if (redo && history.can_redo) || (!redo && history.can_undo) {
                self.gesture = None;
                self.dispatch(if redo {
                    Command::Redo(EmptyParams {})
                } else {
                    Command::Undo(EmptyParams {})
                });
            }
        }
        let edit_key = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Copy => Some(editing::Action::Copy),
                egui::Event::Cut => Some(editing::Action::Cut),
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if (modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
                    && !modifiers.alt
                    && !modifiers.shift =>
                {
                    match key {
                        egui::Key::C => Some(editing::Action::Copy),
                        egui::Key::X => Some(editing::Action::Cut),
                        egui::Key::V => Some(editing::Action::Paste),
                        egui::Key::T => Some(editing::Action::Trim),
                        _ => None,
                    }
                }
                _ => None,
            })
        });
        if let Some(action) = edit_key {
            self.edit_action(action);
        }
        let save_action = ctx.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key: egui::Key::S,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if (modifiers.command || modifiers.ctrl || modifiers.mac_cmd)
                    && !modifiers.alt =>
                {
                    Some(if modifiers.shift {
                        saving::FileAction::SaveAs
                    } else {
                        saving::FileAction::Save
                    })
                }
                _ => None,
            })
        });
        if let Some(action) = save_action {
            if self.host.presentation().mode == ViewMode::Multitrack {
                self.project_save(ctx, matches!(action, saving::FileAction::SaveAs), false);
            } else if let Some(doc) = self.active_document() {
                self.file_action(action, &doc.id, false);
            }
        }
        if pressed(egui::Key::Delete) || pressed(egui::Key::Backspace) {
            self.edit_action(editing::Action::Delete);
        }
        if self.host.presentation().mode == ViewMode::Waveform
            && let Some(document) = self.active_document()
        {
            if select_all {
                self.gesture = None;
                self.dispatch(Command::SelectAll(DocumentParams {
                    document_id: document.id.clone(),
                }));
            }
            if pressed(egui::Key::Escape) {
                if self.gesture.is_some() {
                    self.cancel_gesture();
                } else {
                    self.dispatch(Command::ClearSelection(DocumentParams {
                        document_id: document.id,
                    }));
                }
            }
        }
        let toggle = pressed(egui::Key::Backtick);
        if toggle {
            self.dispatch(Command::TogglePanels(PresentationRevisionParams::default()));
        }
        if pressed(egui::Key::Slash) {
            self.toggle_loop();
        }
        if pressed(egui::Key::Space) {
            self.toggle_playback();
        }
        if pressed(egui::Key::Enter) || pressed(egui::Key::Comma) {
            self.rewind();
        }
    }

    // Independent of eframe::Frame for real egui pointer/key/focus tests.
    fn draw(&mut self, ui: &mut egui::Ui) {
        self.host.begin_frame(ui.ctx());
        self.handle_native_close(ui.ctx());
        if !self.host.frozen() && !ui.ctx().input(|input| input.focused) {
            self.cancel_gesture();
        }
        self.poll_dialog();
        self.file_drops(ui.ctx());
        self.update_waveform();
        self.shortcuts(ui.ctx());
        let presentation = self.host.presentation();
        let mode = presentation.mode;
        if mode != ViewMode::Waveform {
            self.gesture = None;
            self.host.set_transient_resources(Vec::new());
        }
        if mode != ViewMode::Multitrack && !self.host.frozen() {
            self.arrangement_gesture = None;
        }
        let mut help = Help::default();
        let error = self.error.clone().or(self.host.transport().output.error).or_else(||self.host.mix().preparation_error.map(|error|error.message)).or_else(|| match &presentation.preferences {
            PreferenceStatus::Error { .. } => Some("Layout preferences could not be saved or loaded. Check the preferences file and its permissions.".into()),
            _ => None,
        });
        egui::Panel::top("header")
            .exact_size(36.0)
            .frame(panel_frame())
            .show(ui, |ui| {
                let area = ui.max_rect().shrink2(Vec2::new(14.0, 7.0));
                let mut left = child(
                    ui,
                    Rect::from_min_size(area.min, Vec2::new(300.0, area.height())),
                );
                left.horizontal(|ui| {
                    ui.label(RichText::new("WaveBob").strong().size(14.0));
                    ui.add_space(12.0);
                    let menu = ui.menu_button("File", |ui| {
                        let response = ui.button("Open audio…");
                        help.observe(&response, "Open a WAV or MP3 audio file in a new document.");
                        if response.clicked() && !self.host.frozen() {
                            self.open_requested = true;
                            ui.close();
                        }
                        ui.separator();
                        for (label, save_as) in [("Save project    Ctrl/Cmd+S in multitrack", false), ("Save project as…", true)] {
                            let response = ui.button(label);
                            help.observe(&response, "Save documents, lossless working audio and the arrangement as a portable project.");
                            if response.clicked() { self.project_save(ui.ctx(), save_as, false); ui.close(); }
                        }
                        let response = ui.button("Open project…");
                        help.observe(&response, "Open a portable project after saving or discarding current changes.");
                        if response.clicked() { self.choose_project(ui.ctx(), false, false); ui.close(); }
                        ui.separator();
                        for (action,label) in [(saving::FileAction::Save,"Save    Ctrl/Cmd+S"),(saving::FileAction::SaveAs,"Save As…"),(saving::FileAction::Export,"Export audio…")]{
                            let mix=mode==ViewMode::Multitrack&&matches!(action,saving::FileAction::Export);
                            let arrangement=self.host.snapshot().arrangement;
                            let enabled=if mix {arrangement.frames()>0||arrangement.selection.is_some()}else{self.active_document().is_some_and(|doc|doc.frames>0)&&mode==ViewMode::Waveform};
                            let response=ui.add_enabled(enabled,egui::Button::new(label));help.observe(&response,if mix{"Export the selected timeline interval or complete arrangement as WAV or MP3."}else if enabled{"Save the document or export its selection with explicit format and overwrite settings."}else{"Open a nonempty waveform document to enable sample output."});
                            if response.clicked(){if mix{self.begin_mix_export();}else if let Some(doc)=self.active_document(){self.file_action(action,&doc.id,false);}ui.close();}
                        }
                        ui.separator();let response=ui.button("Quit");help.observe(&response,"Quit WaveBob after saving or explicitly discarding dirty work.");if response.clicked(){self.begin_close(saving::CloseTarget::Quit);ui.close();}
                    });
                    help.observe(
                        &menu.response,
                        "Open a WAV or MP3 audio file in a new document.",
                    );
                    let edit = ui.menu_button("Edit", |ui| {
                        let history = self.host.history();
                        for (label, enabled, redo) in [
                            ("Undo    Ctrl/Cmd+Z", history.can_undo, false),
                            ("Redo    Ctrl/Cmd+Shift+Z", history.can_redo, true),
                        ] {
                            let response = ui.add_enabled(enabled, egui::Button::new(label));
                            help.observe(
                                &response,
                                if redo {
                                    "Reapply the last undone edit."
                                } else {
                                    "Restore the state before the last edit."
                                },
                            );
                            if response.clicked() {
                                self.dispatch(if redo {
                                    Command::Redo(EmptyParams {})
                                } else {
                                    Command::Undo(EmptyParams {})
                                });
                                ui.close();
                            }
                        }
                        ui.separator();
                        self.edit_menu(ui, &mut help);
                        ui.separator();
                        let document = self.active_document();
                        let enabled = mode == ViewMode::Waveform && document.is_some();
                        for (label, all) in [
                            ("Select all    Ctrl/Cmd+A", true),
                            ("Clear selection    Escape", false),
                        ] {
                            let response = ui.add_enabled(enabled, egui::Button::new(label));
                            help.observe(
                                &response,
                                if all {
                                    "Select every frame without changing the audio."
                                } else {
                                    "Clear the selection and keep the edit cursor."
                                },
                            );
                            if response.clicked()
                                && let Some(document) = &document
                            {
                                self.gesture = None;
                                self.dispatch(if all {
                                    Command::SelectAll(DocumentParams {
                                        document_id: document.id.clone(),
                                    })
                                } else {
                                    Command::ClearSelection(DocumentParams {
                                        document_id: document.id.clone(),
                                    })
                                });
                                ui.close();
                            }
                        }
                    });
                    help.observe(
                        &edit.response,
                        "Select all or clear the active document's selection.",
                    );
                    for name in ["View", "Help"] {
                        help.observe(
                            &ui.add_enabled(false, egui::Button::new(name).frame(false)),
                            "Menus will become available as editing features are added.",
                        );
                    }
                });
                let tabs = Rect::from_center_size(area.center(), Vec2::new(190.0, 28.0));
                let mut tabs_ui = child(ui, tabs);
                tabs_ui.spacing_mut().item_spacing.x = 0.0;
                tabs_ui.horizontal(|ui| {
                    for (label, selected_mode) in [
                        ("Waveform", ViewMode::Waveform),
                        ("Multitrack", ViewMode::Multitrack),
                    ] {
                        let response = ui.add_sized(
                            [94.0, 26.0],
                            egui::Button::new(label).selected(mode == selected_mode),
                        );
                        help.observe(
                            &response,
                            if selected_mode == ViewMode::Waveform {
                                "Open the waveform editing view."
                            } else {
                                "Open the multitrack arrangement view."
                            },
                        );
                        if response.clicked() {
                            self.set_mode(selected_mode);
                        }
                    }
                });
            });

        // Allocate the strip first; paint its text after collecting this frame's help.
        let status = egui::Panel::bottom("status")
            .exact_size(
                if self
                    .display_document()
                    .is_some_and(|doc| doc.selection.is_some())
                {
                    if error.is_some() { 74.0 } else { 50.0 }
                } else if error.is_some() {
                    54.0
                } else {
                    30.0
                },
            )
            .frame(panel_frame())
            .show(ui, |ui| ui.max_rect())
            .inner;
        let [left_width, right_width] = presentation.layout.effective_widths(ui.available_width());
        let full_width = ui.available_width();
        egui::Panel::left("library").exact_size(left_width).resizable(false).frame(panel_frame()).show(ui, |ui| {
            let rect = ui.max_rect();
            if presentation.layout.left.visible {
                self.library(ui, rect.shrink2(Vec2::new(12.0, 10.0)), &mut help, &presentation.layout);
                let handle = Rect::from_min_max(Pos2::new(rect.right()-6.0, rect.top()), rect.max);
                let response = ui.interact(handle, ui.scope_id().with("resize-left"), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                help.observe(&response, "Drag to resize the file panel.");
                if response.dragged() && let Some(pointer) = response.interact_pointer_pos() {
                    let other = if presentation.layout.right.visible { MIN_PANEL_WIDTH } else { COLLAPSED_WIDTH };
                    self.layout(SetLayoutParams { left_width: Some((pointer.x - rect.left()).clamp(MIN_PANEL_WIDTH, (full_width - MIN_CENTER_WIDTH - other).max(MIN_PANEL_WIDTH)) as f64), ..Default::default() });
                }
            } else {
                let response = ui.put(Rect::from_min_size(rect.min+Vec2::new(1.0, 10.0), Vec2::new(22.0, 26.0)), egui::Button::new("›").frame(false));
                help.observe(&response, "Restore the file panel at its previous width. Backtick toggles both panels.");
                if response.clicked() { self.layout(SetLayoutParams { left_visible: Some(true), ..Default::default() }); }
            }
        });
        egui::Panel::right("effects").exact_size(right_width).resizable(false).frame(panel_frame()).show(ui, |ui| {
            let rect = ui.max_rect();
            if presentation.layout.right.visible {
                self.effects(ui, rect.shrink2(Vec2::new(12.0, 10.0)), &mut help);
                let handle = Rect::from_min_size(rect.min, Vec2::new(6.0, rect.height()));
                let response = ui.interact(handle, ui.scope_id().with("resize-right"), Sense::drag()).on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                help.observe(&response, "Drag to resize the effects panel.");
                if response.dragged() && let Some(pointer) = response.interact_pointer_pos() {
                    let other = if presentation.layout.left.visible { MIN_PANEL_WIDTH } else { COLLAPSED_WIDTH };
                    self.layout(SetLayoutParams { right_width: Some((rect.right()-pointer.x).clamp(MIN_PANEL_WIDTH, (full_width - MIN_CENTER_WIDTH - other).max(MIN_PANEL_WIDTH)) as f64), ..Default::default() });
                }
            } else {
                let response = ui.put(Rect::from_min_size(rect.min+Vec2::new(1.0, 10.0), Vec2::new(22.0, 26.0)), egui::Button::new("‹").frame(false));
                help.observe(&response, "Restore the effects panel at its previous width. Backtick toggles both panels.");
                if response.clicked() { self.layout(SetLayoutParams { right_visible: Some(true), ..Default::default() }); }
            }
        });
        egui::Panel::bottom("transport").exact_size(48.0).frame(panel_frame()).show(ui,|ui| {
            let output=self.host.transport();
            let document=self.active_document();
            let arrangement=self.host.snapshot().arrangement;
            let mix=self.host.mix();
            let target=if mode==ViewMode::Waveform {document.as_ref().is_some_and(|doc|doc.frames>0)} else {arrangement.frames()>0};
            let ready=output.output.status==wavebob::transport::OutputStatus::Ready;
            let playing=output.requested.playing||(mode==ViewMode::Multitrack&&mix.requested_playing);
            let area=ui.max_rect();
            let group=Rect::from_center_size(area.center(),Vec2::new(340.0,32.0));
            let mut controls=child(ui,group);
            controls.horizontal(|ui| {
                let rewind=icon_button(ui,Icon::Start,"Rewind to start",false,target);
                help.observe(&rewind,if target {"Rewind to absolute start and preserve playing/paused state and selection. Shortcut: Enter or comma."} else {"Open audio or arrange a clip to enable rewind. Shortcut: Enter or comma."});
                if rewind.clicked() {self.rewind();}
                if output.output.status==wavebob::transport::OutputStatus::Failed {
                    let retry=ui.button("Retry output");
                    help.observe(&retry,"Retry audio output after connecting or restoring a device.");
                    if retry.clicked() {self.dispatch(Command::RetryOutput(EmptyParams{}));}
                } else {
                    let play=icon_button(ui,if playing {Icon::Pause} else {Icon::Play},if playing {"Pause"} else {"Play"},true,ready && (target || playing));
                    help.observe(&play,if !target && !playing {"Playback is unavailable: open audio or arrange a clip. Shortcut: Space."} else if !ready {"Playback is unavailable while audio output initializes or in a session without an output adapter."} else {"Play or pause from the current cursor. Loop follows the time selection or complete audio. Shortcut: Space."});
                    if play.clicked() {self.toggle_playback();}
                }
                let response=icon_button(ui,Icon::Loop,"Loop",self.host.snapshot().loop_enabled,true);
                help.observe(&response,"Toggle looping without starting playback or changing selection. Shortcut: /.");
                if response.clicked() {self.toggle_loop();}
                ui.add_space(12.0);
                let timing=if mode==ViewMode::Multitrack {format!("{}  /  {}",time(arrangement.cursor,arrangement.sample_rate),time(arrangement.frames(),arrangement.sample_rate))}else{self.active_document().map(|doc|format!("{}  /  {}",time(doc.cursor,doc.sample_rate),time(doc.frames,doc.sample_rate))).unwrap_or_else(||"—  /  —".into())};
                ui.label(RichText::new(timing).monospace().size(12.0));
            });
            if mode==ViewMode::Multitrack&&area.width()>650.0 {
                let meter=Rect::from_min_size(area.min+Vec2::new(14.0,9.0),Vec2::new(130.0,8.0));
                let peak=mix.peak_level;let fraction=if peak>0.0 {((20.0*peak.log10()+60.0)/60.0).clamp(0.0,1.0) as f32}else{0.0};
                ui.painter().rect_filled(meter,2.0,SURFACE);ui.painter().rect_filled(Rect::from_min_size(meter.min,Vec2::new(meter.width()*fraction,meter.height())),2.0,if mix.overloaded{Color32::LIGHT_RED}else{visuals::TEAL});
                let label=if mix.requested_playing&&mix.pending_job_id.is_some(){"Preparing mix…".into()}else if mix.overloaded {format!("Master overload {:+.1} dB",20.0*peak.log10())}else if peak>0.0 {format!("Master {:+.1} dB",20.0*peak.log10())}else{"Master −∞ dB".into()};
                ui.painter().text(meter.left_bottom()+Vec2::new(0.0,5.0),egui::Align2::LEFT_TOP,label,egui::FontId::proportional(10.0),if mix.overloaded{Color32::LIGHT_RED}else{MUTED});
                let response=ui.interact(meter.expand(4.0),ui.scope_id().with("master-meter"),Sense::hover());help.observe(&response,"Master output peak before device clipping. Red indicates floating-point overload; lower track or master gain.");
            }
            if area.width()>980.0 {
                let mut hints=child(ui,Rect::from_min_max(Pos2::new(area.right()-300.0,area.top()+13.0),area.max));
                hints.horizontal(|ui| {ui.label(RichText::new("Space  Play / pause    Enter or ,  Start    /  Loop").size(10.0).color(MUTED));});
            }
        });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgb(14, 19, 25))
                    .inner_margin(14.0),
            )
            .show(ui, |ui| {
                if mode == ViewMode::Waveform {
                    self.waveform_view(ui, &mut help);
                } else {
                    self.multitrack_view(ui, &mut help);
                }
            });
        self.file_modals(ui.ctx(), &mut help);
        self.file_drop_feedback(ui.ctx(), &mut help);
        self.status(ui, status, mode, &help, error.as_deref());
        self.host.set_ui_resources(
            [
                self.output_dialog.is_some().then_some("output-dialog"),
                self.save_picker.is_some().then_some("native-output-picker"),
                self.project_picker
                    .is_some()
                    .then_some("native-project-picker"),
                self.dialog.is_some().then_some("native-open-dialog"),
                self.close_flow.is_some().then_some("dirty-close-dialog"),
                (!self.file_imports.paths.is_empty()).then_some("file-drop-queue"),
                ui.ctx()
                    .input(|input| !input.raw.hovered_files.is_empty())
                    .then_some("external-file-hover"),
            ]
            .into_iter()
            .flatten()
            .map(str::to_owned)
            .collect(),
        );
        self.effect_windows(ui.ctx());
        self.host.end_frame(ui.ctx());
        if self.host.ready_to_close() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let transport = self.host.transport();
        if transport.requested.playing
            || transport.applied.playing
            || transport.pending
            || transport.output.status == wavebob::transport::OutputStatus::Initializing
            || self.close_flow.is_some()
            || self.host.presentation().close_requested
            || self.host.mix().requested_playing
            || !self.file_imports.paths.is_empty()
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }

    fn library(&mut self, ui: &mut egui::Ui, area: Rect, help: &mut Help, layout: &PanelLayout) {
        let split_y = area.top() + area.height() * layout.library_fraction as f32;
        let upper = Rect::from_min_max(area.min, Pos2::new(area.right(), split_y - 4.0));
        let lower = Rect::from_min_max(Pos2::new(area.left(), split_y + 4.0), area.max);
        let mut library = child(ui, upper);
        let (heading, _) =
            library.allocate_exact_size(Vec2::new(upper.width(), 24.0), Sense::hover());
        library.painter().text(
            heading.left_center(),
            egui::Align2::LEFT_CENTER,
            "OPEN FILES",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(217, 226, 240),
        );
        let add = library.put(
            Rect::from_min_size(
                Pos2::new(heading.right() - 48.0, heading.top()),
                Vec2::new(24.0, 24.0),
            ),
            egui::Button::new("+").frame(false),
        );
        help.observe(&add, "Open a WAV or MP3 audio file in a new document.");
        if add.clicked() && !self.host.frozen() {
            self.open_requested = true;
        }
        let hide = library.put(
            Rect::from_min_size(
                Pos2::new(heading.right() - 22.0, heading.top()),
                Vec2::new(22.0, 24.0),
            ),
            egui::Button::new("‹").frame(false),
        );
        help.observe(&hide, "Hide the file panel. Backtick toggles both panels.");
        if hide.clicked() {
            self.layout(SetLayoutParams {
                left_visible: Some(false),
                ..Default::default()
            });
        }
        egui::ScrollArea::vertical()
            .id_salt("open-files")
            .show(&mut library, |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let snapshot = self.host.snapshot();
                if snapshot.documents.is_empty() {
                    ui.label(
                        RichText::new("No audio documents open")
                            .small()
                            .color(MUTED),
                    );
                    let response = ui.button("Open audio…");
                    help.observe(&response, "Open a WAV or MP3 audio file in a new document.");
                    if response.clicked() && !self.host.frozen() {
                        self.open_requested = true;
                    }
                }
                for document in snapshot.documents {
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - 29.0).max(30.0);
                        let response = ui
                            .allocate_ui_with_layout(
                                Vec2::new(width, 24.0),
                                egui::Layout::top_down(Align::Min),
                                |ui| {
                                    visuals::file_row(
                                        ui,
                                        &if document.dirty {
                                            format!("{} *", document.name)
                                        } else {
                                            document.name.clone()
                                        },
                                        snapshot.active_document_id.as_ref() == Some(&document.id),
                                        false,
                                        true,
                                    )
                                },
                            )
                            .inner;
                        let response=response.interact(Sense::click_and_drag());
                        if response.drag_started() && self.host.presentation().mode==ViewMode::Multitrack && !self.host.frozen() {
                            self.arrangement_gesture=Some(arranging::Gesture::Insert{document:document.clone(),revision:self.host.snapshot().revision});
                        }
                        help.observe(
                            &response,
                            "Click to edit this waveform. Drag onto a track in multitrack mode to insert a pinned clip.",
                        );
                        if response.clicked() {
                            self.dispatch(Command::ActivateDocument(DocumentParams {
                                document_id: document.id.clone(),
                            }));
                            self.set_mode(ViewMode::Waveform);
                        }
                        let close = ui.add_sized([22.0, 22.0], egui::Button::new("×").frame(false));
                        help.observe(&close, "Close this audio document.");
                        if close.clicked() {
                            self.begin_close(saving::CloseTarget::Document(document.id.clone()));
                        }
                    });
                }
                for job in snapshot
                    .jobs
                    .iter()
                    .filter(|job| job.status == JobStatus::Running)
                {
                    ui.add(
                        egui::ProgressBar::new(job.progress as f32 / 100.0).text(format!(
                            "{}… {}%",
                            if job.kind == "import" {
                                "Opening"
                            } else {
                                &job.kind
                            },
                            job.progress
                        )),
                    );
                    let response = ui.small_button(if job.kind == "import" {
                        "Cancel import"
                    } else {
                        "Cancel edit"
                    });
                    help.observe(
                        &response,
                        "Cancel this job without publishing partial audio.",
                    );
                    if response.clicked() {
                        self.dispatch(Command::CancelJob(JobParams {
                            job_id: job.id.clone(),
                        }));
                    }
                }
                if !self.file_imports.paths.is_empty() {
                    ui.label(RichText::new(format!("{} files queued",self.file_imports.paths.len())).small().color(MUTED));
                    let response=ui.small_button("Cancel queued");
                    help.observe(&response,"Cancel files that have not started opening. Running imports have their own cancel buttons.");
                    if response.clicked()&&!self.host.frozen(){self.file_imports.clear();}
                }
            });
        let handle = Rect::from_min_max(
            Pos2::new(area.left(), split_y - 4.0),
            Pos2::new(area.right(), split_y + 4.0),
        );
        let response = ui
            .interact(handle, ui.scope_id().with("library-split"), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
        ui.painter().hline(
            handle.x_range(),
            handle.center().y,
            Stroke::new(1.0, BORDER),
        );
        help.observe(
            &response,
            "Drag to divide the open-file list and file browser.",
        );
        if response.dragged()
            && let Some(pointer) = response.interact_pointer_pos()
        {
            self.layout(SetLayoutParams {
                library_fraction: Some(
                    ((pointer.y - area.top()) / area.height()).clamp(0.15, 0.85) as f64,
                ),
                ..Default::default()
            });
        }
        self.browser_view(ui, lower, help);
    }

    fn status(
        &self,
        ui: &mut egui::Ui,
        rect: Rect,
        mode: ViewMode,
        help: &Help,
        error: Option<&str>,
    ) {
        let mut content = child(ui, rect.shrink2(Vec2::new(12.0, 4.0)));
        if let Some(error) = error {
            content.label(
                RichText::new(error)
                    .size(11.0)
                    .color(Color32::from_rgb(255, 180, 128)),
            );
        }
        let document = if mode == ViewMode::Waveform {
            self.display_document()
        } else {
            None
        };
        let selected = document.as_ref().is_some_and(|doc| doc.selection.is_some());
        let readout = if mode == ViewMode::Waveform {
            document
                .map(|doc| {
                    if let Some(selection) = doc.selection {
                        format!(
                            "Cursor {} · Selection {}–{}  |  {}–{}  |  {} frames / {}",
                            exact_time(doc.cursor, doc.sample_rate),
                            selection.start,
                            selection.end,
                            exact_time(selection.start, doc.sample_rate),
                            exact_time(selection.end, doc.sample_rate),
                            selection.end - selection.start,
                            exact_time(selection.end - selection.start, doc.sample_rate)
                        )
                    } else {
                        format!(
                            "Cursor {} ({}) · No selection · {} frames",
                            exact_time(doc.cursor, doc.sample_rate),
                            doc.cursor,
                            doc.frames
                        )
                    }
                })
                .unwrap_or_else(|| "No document · Cursor — · No selection".into())
        } else {
            let arrangement = self.host.snapshot().arrangement;
            if arrangement.tracks.is_empty() {
                "Empty arrangement · Selection: none · Selected clips: 0".into()
            } else {
                let selection = arrangement
                    .selection
                    .as_ref()
                    .map_or("No selection".into(), |r| {
                        format!(
                            "Selection {}–{} · {} frames",
                            exact_time(r.start, arrangement.sample_rate),
                            exact_time(r.end, arrangement.sample_rate),
                            r.end - r.start
                        )
                    });
                format!(
                    "Cursor {} · {} · Selected clips: {}",
                    exact_time(arrangement.cursor, arrangement.sample_rate),
                    selection,
                    arrangement.selected_clip_ids.len()
                )
            }
        };
        let saving = matches!(
            self.host.presentation().preferences,
            PreferenceStatus::Saving
        );
        content.horizontal(|ui| {
            ui.label(RichText::new(readout).size(11.0));
            if !selected {
                ui.separator();
                let available = (ui.available_width() - if saving { 95.0 } else { 0.0 }).max(0.0);
                ui.add_sized(
                    [available, 18.0],
                    egui::Label::new(RichText::new(help.text()).size(11.0).color(MUTED)).truncate(),
                );
            }
            if saving {
                ui.label(RichText::new("Saving layout…").size(10.0).color(MUTED));
            }
        });
        if selected {
            content.add(
                egui::Label::new(RichText::new(help.text()).size(11.0).color(MUTED)).truncate(),
            );
        }
    }
}

/// Native viewports share the same UI model while rendering independently.
/// This mutex never enters the audio callback.
pub struct DesktopApp {
    state: std::sync::Arc<std::sync::Mutex<WaveBob>>,
}
impl DesktopApp {
    pub fn new(app: WaveBob) -> Self {
        let state = std::sync::Arc::new(std::sync::Mutex::new(app));
        state.lock().unwrap().shared_self = std::sync::Arc::downgrade(&state);
        Self { state }
    }
}
impl eframe::App for DesktopApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        eframe::App::raw_input_hook(&mut *self.state.lock().unwrap(), ctx, input);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        eframe::App::ui(&mut *self.state.lock().unwrap(), ui, frame);
    }
}

impl eframe::App for WaveBob {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.text_clipboard_input(ctx, input);
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
        self.start_dialog(ui.ctx());
    }
}

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
}
fn encoding_label(document: &DocumentInfo) -> String {
    if let Some(format) = &document.save_format {
        use wavebob::exports::{ExportFormat, WavEncoding};
        return match format {
            ExportFormat::Mp3 { bitrate_kbps } => format!("MP3 CBR {bitrate_kbps}"),
            ExportFormat::Wav { encoding } => match encoding {
                WavEncoding::Pcm16 => "16-bit",
                WavEncoding::Pcm24 => "24-bit",
                WavEncoding::Pcm32 => "32-bit",
                WavEncoding::Float32 => "float 32-bit",
            }
            .into(),
        };
    }
    if document.encoding == "mp3" {
        format!(
            "MP3 {}",
            document
                .source_details
                .bitrate_mode
                .as_deref()
                .unwrap_or("")
        )
    } else {
        document
            .encoding
            .trim_start_matches("wav_")
            .replace("pcm", "")
            .replace("float", "float ")
            .to_string()
            + "-bit"
    }
}
fn exact_time(frame: u64, rate: u32) -> String {
    let micros = frame.saturating_mul(1_000_000) / rate.max(1) as u64;
    format!(
        "{:02}:{:02}.{:06}",
        micros / 60_000_000,
        (micros / 1_000_000) % 60,
        micros % 1_000_000
    )
}
fn time(frame: u64, rate: u32) -> String {
    let millis = frame.saturating_mul(1000) / rate.max(1) as u64;
    format!(
        "{:02}:{:02}.{:03}",
        millis / 60000,
        (millis / 1000) % 60,
        millis % 1000
    )
}
fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong());
}
fn centered_button(text: &str) -> egui::Button<'_> {
    egui::Button::new((egui::Atom::grow(), text, egui::Atom::grow()))
}
fn child(ui: &mut egui::Ui, rect: Rect) -> egui::Ui {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    child
}

fn theme(ctx: &egui::Context) {
    ctx.options_mut(|options| {
        options.input_options.zoom_modifier = egui::Modifiers {
            ctrl: true,
            mac_cmd: true,
            command: true,
            ..Default::default()
        }
    });
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/InterVariable.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "Inter".into());
    ctx.set_fonts(fonts);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = SURFACE;
    visuals.window_fill = SURFACE;
    visuals.override_text_color = Some(Color32::from_rgb(209, 220, 235));
    visuals.selection.bg_fill = BLUE;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.slider_trailing_fill = true;
    visuals.handle_shape = egui::style::HandleShape::Circle;
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(28, 36, 45);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    ctx.set_visuals(visuals);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(7.0, 4.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.explanation_tooltips = false;
        style.url_in_tooltip = false;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(12.0));
        style
            .text_styles
            .insert(egui::TextStyle::Small, egui::FontId::proportional(11.0));
    });
}

#[cfg(test)]
#[path = "app/tests.rs"]
mod tests;
