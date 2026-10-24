use super::*;
use wavebob::effects::EditTarget;
use wavebob::exports::{ExportFormat, ExportParams, OverloadPolicy, SaveParams, WavEncoding};
#[derive(Clone, Copy)]
pub(super) enum FileAction {
    Save,
    SaveAs,
    Export,
}
#[derive(Clone)]
pub(super) enum CloseTarget {
    Document(String),
    Quit,
    OpenProject(String),
}
pub(super) struct CloseFlow {
    target: CloseTarget,
    pub(super) saving: bool,
    discard: bool,
    cancelled_jobs: bool,
    save_authorized: bool,
}
pub(super) struct OutputDialog {
    mix: bool,
    document: DocumentInfo,
    save: bool,
    selection: bool,
    path: String,
    format: ExportFormat,
    rate: Option<u32>,
    overwrite: bool,
    overload: OverloadPolicy,
    close_after: bool,
}
impl OutputDialog {
    fn range(&self) -> FrameRange {
        if self.selection {
            self.document.selection.clone().unwrap()
        } else {
            FrameRange {
                start: 0,
                end: self.document.frames,
            }
        }
    }
}

impl WaveBob {
    pub(super) fn begin_mix_export(&mut self) {
        if self.host.frozen() {
            return;
        }
        let arrangement = self.host.snapshot().arrangement;
        if arrangement.frames() == 0 && arrangement.selection.is_none() {
            self.error =
                Some("Arrange audio or select a nonempty timeline interval before export.".into());
            return;
        }
        self.error = None;
        let path = std::env::current_dir()
            .unwrap_or_default()
            .join("Arrangement.wav")
            .to_string_lossy()
            .into_owned();
        let frames = arrangement.frames();
        let document = DocumentInfo {
            processing: wavebob::processing::Processing::default(),
            saved_processing: wavebob::processing::Processing::default(),
            tab_open: true,
            id: arrangement.id,
            revision_id: arrangement.revision_id,
            path: path.clone(),
            name: "Arrangement".into(),
            sample_rate: arrangement.sample_rate,
            channels: 2,
            frames,
            encoding: "float32".into(),
            source_details: Default::default(),
            dirty: arrangement.dirty,
            cursor: arrangement.cursor,
            selection: arrangement.selection,
            peak_level: 0.0,
            overloaded_samples: 0,
            saved_revision_id: None,
            save_format: None,
            save_sample_rate: None,
        };
        self.output_dialog = Some(OutputDialog {
            mix: true,
            selection: document.selection.is_some(),
            document,
            save: false,
            path,
            format: ExportFormat::default(),
            rate: None,
            overwrite: false,
            overload: OverloadPolicy::Reject,
            close_after: false,
        });
    }
    pub(super) fn file_action(&mut self, mut action: FileAction, id: &str, close_after: bool) {
        if self.host.frozen() {
            return;
        }
        let Some(document) = self
            .host
            .snapshot()
            .documents
            .into_iter()
            .find(|doc| doc.id == id)
        else {
            return;
        };
        if document.frames == 0 {
            self.error = Some(
                "Empty samples cannot be saved or exported. Paste audio or undo the deletion."
                    .into(),
            );
            return;
        }
        self.error = None;
        if self.host.snapshot().project.path.is_some()
            && !std::path::Path::new(&document.path).is_absolute()
            && matches!(action, FileAction::Save)
        {
            action = FileAction::SaveAs;
        }
        let existing = document
            .save_format
            .clone()
            .or_else(|| ExportFormat::source_wav(&document.encoding));
        if matches!(action, FileAction::Save)
            && let Some(format) = &existing
            && !format.lossy()
            && (document.overloaded_samples == 0
                || matches!(
                    format,
                    ExportFormat::Wav {
                        encoding: WavEncoding::Float32
                    }
                ))
        {
            self.submit_output(
                Command::SaveDocument(SaveParams {
                    document_id: document.id,
                    revision_id: document.revision_id,
                    path: None,
                    format: None,
                    sample_rate: None,
                    overwrite: true,
                    overload: OverloadPolicy::Reject,
                }),
                close_after,
            );
            return;
        }
        let save = !matches!(action, FileAction::Export);
        let format = if matches!(action, FileAction::Save) {
            existing.unwrap_or(ExportFormat::Mp3 { bitrate_kbps: 192 })
        } else {
            ExportFormat::default()
        };
        let mut path = std::path::PathBuf::from(&document.path);
        if !matches!(action, FileAction::Save) {
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            path.set_file_name(format!("{stem}-edited.{}", format.extension()));
        } else {
            path.set_extension(format.extension());
        }
        let rate = if matches!(action, FileAction::Save) {
            document.save_sample_rate
        } else {
            None
        };
        self.output_dialog = Some(OutputDialog {
            mix: false,
            selection: !save && document.selection.is_some(),
            document,
            save,
            path: path.to_string_lossy().into_owned(),
            format,
            rate,
            overwrite: matches!(action, FileAction::Save),
            overload: OverloadPolicy::Reject,
            close_after,
        });
    }
    pub(super) fn submit_output(&mut self, command: Command, close_after: bool) -> bool {
        if self.host.frozen() {
            return false;
        }
        match self
            .host
            .execute(Request::new("gui-output", command))
            .outcome
        {
            Outcome::Ok {
                result: CommandResult::Job(job),
            } => {
                if close_after {
                    self.close_save_job = Some(job.id);
                }
                self.error = None;
                true
            }
            Outcome::Error { error } => {
                self.error = Some(error.message);
                false
            }
            _ => false,
        }
    }
    pub(super) fn begin_close(&mut self, target: CloseTarget) {
        if self.host.frozen() {
            return;
        }
        if matches!(target, CloseTarget::Quit | CloseTarget::OpenProject(_)) {
            self.file_imports.clear();
        }
        let snapshot = self.host.snapshot();
        let dirty = match &target {
            CloseTarget::Quit | CloseTarget::OpenProject(_) => snapshot.project.dirty,
            CloseTarget::Document(id) => snapshot
                .documents
                .iter()
                .any(|doc| &doc.id == id && self.document_unsaved(doc)),
        };
        if !dirty && let CloseTarget::Document(id) = &target {
            self.dispatch(Command::CloseDocument(CloseParams {
                document_id: id.clone(),
                discard_changes: false,
            }));
            return;
        }
        self.close_flow = Some(CloseFlow {
            target,
            saving: !dirty,
            discard: false,
            cancelled_jobs: false,
            save_authorized: false,
        });
    }
    pub(super) fn handle_native_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|input| input.viewport().close_requested())
            && !self.host.presentation().close_requested
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.os_close_pending = true;
        }
        if self.os_close_pending && !self.host.frozen() {
            self.os_close_pending = false;
            self.begin_close(CloseTarget::Quit);
        }
    }
    fn poll_save_picker(&mut self) {
        if let Some(receiver) = &self.save_picker {
            match receiver.try_recv() {
                Ok(path) => {
                    self.save_picker = None;
                    if let Some(path) = path
                        && let Some(dialog) = &mut self.output_dialog
                    {
                        match path.into_os_string().into_string() {
                            Ok(path) => {
                                dialog.path = path;
                                dialog.overwrite = true;
                            }
                            Err(_) => {
                                self.error = Some("The selected path is not valid Unicode.".into())
                            }
                        }
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.save_picker = None;
                    self.error = Some("The save dialog did not complete.".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
    }
    fn choose_output_path(&mut self, ctx: &egui::Context, dialog: &OutputDialog) {
        if self.save_picker.is_some() || self.host.frozen() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.save_picker = Some(receiver);
        let path = std::path::PathBuf::from(&dialog.path);
        let extension = dialog.format.extension();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let mut picker = rfd::FileDialog::new()
                .set_title("Choose audio output")
                .add_filter("Audio output", &[extension]);
            if let Some(parent) = path.parent() {
                picker = picker.set_directory(parent);
            }
            if let Some(name) = path.file_name() {
                picker = picker.set_file_name(name.to_string_lossy());
            }
            let _ = sender.send(picker.save_file());
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
            ctx.request_repaint();
        });
    }
    fn output_modal(&mut self, ctx: &egui::Context, help: &mut Help) {
        let Some(mut dialog) = self.output_dialog.take() else {
            return;
        };
        let mut dismiss = false;
        let mut submit = false;
        let modal=egui::Modal::new(egui::Id::unique("audio-output-dialog")).show(ctx,|ui|{
            ui.set_max_width(480.0);ui.heading(if dialog.mix{"Export arrangement"}else if dialog.save{"Save audio"}else{"Export audio"});ui.label(&dialog.document.name);
            if !dialog.save&&dialog.document.selection.is_some(){let response=ui.checkbox(&mut dialog.selection,"Export selection only");help.observe(&response,if dialog.mix{"Export the timeline selection; turn off for the complete arrangement."}else{"Export the selected interval; turn off to export the whole sample."});}
            let response=ui.add(egui::TextEdit::singleline(&mut dialog.path).desired_width(430.0).hint_text("Output path"));help.observe(&response,"Output path. The extension must match the selected format.");
            let response=ui.button("Browse…");help.observe(&response,"Choose a filename using the native save dialog.");if response.clicked(){self.choose_output_path(ctx,&dialog);}
            let old_format=dialog.format.clone();
            ui.horizontal(|ui|{ui.label("Format");let response=egui::ComboBox::from_id_salt("output-format").selected_text(match &dialog.format{ExportFormat::Wav{encoding}=>match encoding{WavEncoding::Pcm16=>"WAV · 16-bit PCM",WavEncoding::Pcm24=>"WAV · 24-bit PCM",WavEncoding::Pcm32=>"WAV · 32-bit PCM",WavEncoding::Float32=>"WAV · 32-bit float"},ExportFormat::Mp3{..}=>"MP3 · lossy"}).show_ui(ui,|ui|{
                for (encoding,label) in [(WavEncoding::Pcm16,"WAV · 16-bit PCM"),(WavEncoding::Pcm24,"WAV · 24-bit PCM"),(WavEncoding::Pcm32,"WAV · 32-bit PCM"),(WavEncoding::Float32,"WAV · 32-bit float")]{let response=ui.selectable_value(&mut dialog.format,ExportFormat::Wav{encoding},label);help.observe(&response,"Choose lossless WAV storage. Float WAV preserves finite headroom.");}
                let response=ui.selectable_label(dialog.format.lossy(),"MP3 · lossy");help.observe(&response,"Choose lossy MP3 with explicit bitrate and output rate settings.");if response.clicked(){dialog.format=ExportFormat::Mp3{bitrate_kbps:192};}
            });help.observe(&response.response,"Choose WAV encoding or lossy MP3. New exports default to 24-bit WAV.");});
            if old_format.extension()!=dialog.format.extension(){let mut path=std::path::PathBuf::from(&dialog.path);path.set_extension(dialog.format.extension());dialog.path=path.to_string_lossy().into_owned();}
            if let ExportFormat::Mp3{bitrate_kbps}=&mut dialog.format{ui.horizontal(|ui|{ui.label("CBR bitrate");let response=egui::ComboBox::from_id_salt("mp3-bitrate").selected_text(format!("{bitrate_kbps} kbit/s")).show_ui(ui,|ui|{for bitrate in [128,192,256,320]{let response=ui.selectable_value(bitrate_kbps,bitrate,format!("{bitrate} kbit/s"));help.observe(&response,"Choose the MP3 CBR bitrate in kilobits per second.");}});help.observe(&response.response,"MP3 CBR bitrate. The working audio revision remains lossless.");});ui.label(RichText::new("MP3 uses lossy encoding. Working audio is retained.").small().color(Color32::from_rgb(226,178,89)));}
            ui.horizontal(|ui|{ui.label("Sample rate");let response=egui::ComboBox::from_id_salt("output-rate").selected_text(dialog.rate.map_or("Automatic".into(),|rate|format!("{rate} Hz"))).show_ui(ui,|ui|{let response=ui.selectable_value(&mut dialog.rate,None,"Automatic");help.observe(&response,"Preserve a supported source rate; MP3 otherwise uses 44.1 kHz.");for rate in [32000,44100,48000]{let response=ui.selectable_value(&mut dialog.rate,Some(rate),format!("{rate} Hz"));help.observe(&response,"Convert the output to this sample rate using the shared resampler.");}});help.observe(&response.response,"Automatic preserves supported source rates; MP3 otherwise defaults to 44.1 kHz. Conversion uses the shared resampler.");});
            let range=dialog.range();
            if dialog.mix {ui.label(format!("Project frames {}–{} · {:.6}–{:.6} s",range.start,range.end,range.start as f64/dialog.document.sample_rate as f64,range.end as f64/dialog.document.sample_rate as f64));ui.label(RichText::new("Exact range · no tail beyond clip/range boundaries").small().color(MUTED));}
            if let Ok(rate)=dialog.format.output_rate(dialog.document.sample_rate,dialog.rate){let frames=wavebob::exports::converted_frames(range.end-range.start,dialog.document.sample_rate,rate).unwrap_or(0);ui.label(format!("{} frames · {} Hz · {}{}",frames,rate,if dialog.document.channels==1{"Mono"}else{"Stereo"},if rate!=dialog.document.sample_rate{" · resampled"}else{""}));}
            let response=ui.checkbox(&mut dialog.overwrite,"Allow replacing an existing file");help.observe(&response,"Existing output is replaced atomically only with this explicit intent.");
            let mut clip=dialog.overload==OverloadPolicy::Clip;let response=ui.checkbox(&mut clip,"Clip overloads for integer WAV / MP3");help.observe(&response,"Off rejects overloads. Float WAV preserves finite headroom. Clipped sample counts are reported in the job result.");dialog.overload=if clip{OverloadPolicy::Clip}else{OverloadPolicy::Reject};
            if let Some(error)=&self.error{ui.colored_label(Color32::LIGHT_RED,error);}
            ui.add_space(8.0);ui.horizontal(|ui|{let response=ui.add_enabled(self.save_picker.is_none(),egui::Button::new(if dialog.save{"Save"}else{"Export"}).fill(BLUE));help.observe(&response,if self.save_picker.is_some(){"Finish the native file chooser to enable output."}else{"Write the displayed interval and format as a background job."});submit=response.clicked();let response=ui.button("Cancel");help.observe(&response,"Close these output settings without starting a job.");dismiss=response.clicked();});
        });
        if modal.should_close() && !self.host.frozen() {
            dismiss = true;
        }
        if submit && !self.host.frozen() {
            let command = if dialog.mix {
                let range = dialog.range();
                Command::ExportMix(wavebob::exports::MixExportParams {
                    revision_id: dialog.document.revision_id.clone(),
                    start_frame: range.start,
                    end_frame: range.end,
                    path: dialog.path.clone(),
                    format: dialog.format.clone(),
                    sample_rate: dialog.rate,
                    overwrite: dialog.overwrite,
                    overload: dialog.overload,
                })
            } else if dialog.save {
                Command::SaveDocument(SaveParams {
                    document_id: dialog.document.id.clone(),
                    revision_id: dialog.document.revision_id.clone(),
                    path: Some(dialog.path.clone()),
                    format: Some(dialog.format.clone().into()),
                    sample_rate: dialog.rate,
                    overwrite: dialog.overwrite,
                    overload: dialog.overload,
                })
            } else {
                let target = if dialog.selection {
                    let range = dialog.document.selection.as_ref().unwrap();
                    EditTarget::Range {
                        document_id: dialog.document.id.clone(),
                        revision_id: dialog.document.revision_id.clone(),
                        start_frame: range.start,
                        end_frame: range.end,
                    }
                } else {
                    EditTarget::Whole {
                        document_id: dialog.document.id.clone(),
                        revision_id: dialog.document.revision_id.clone(),
                    }
                };
                Command::ExportAudio(ExportParams {
                    target,
                    path: dialog.path.clone(),
                    format: dialog.format.clone(),
                    sample_rate: dialog.rate,
                    overwrite: dialog.overwrite,
                    overload: dialog.overload,
                })
            };
            if self.submit_output(command, dialog.close_after) {
                return;
            }
        }
        if dismiss && !self.host.frozen() {
            if dialog.close_after
                && let Some(flow) = &mut self.close_flow
            {
                flow.saving = false;
            }
            return;
        }
        self.output_dialog = Some(dialog);
    }
    pub(super) fn file_modals(&mut self, ctx: &egui::Context, help: &mut Help) {
        let frozen = self.host.frozen();
        if !frozen {
            self.poll_save_picker();
            self.poll_project_picker();
        }
        if !frozen
            && let Some(id) = &self.close_save_job
            && let Some(job) = self
                .host
                .snapshot()
                .jobs
                .into_iter()
                .find(|job| &job.id == id)
            && job.status != JobStatus::Running
        {
            self.close_save_job = None;
            let changed = matches!(job.output, Some(JobOutput::ProjectSaved(_)))
                && self.host.snapshot().project.dirty
                || job.result.is_some_and(|doc| self.document_unsaved(&doc));
            if job.status != JobStatus::Succeeded || changed {
                self.error = Some(if changed {
                    "Work changed during the save. Choose Save again to review the current revision.".into()
                } else {
                    job.error
                        .map_or("Save was cancelled".into(), |error| error.message)
                });
                if let Some(flow) = &mut self.close_flow {
                    flow.saving = false;
                }
            }
        }
        if self.output_dialog.is_some() {
            self.output_modal(ctx, help);
            return;
        }
        let Some(mut flow) = self.close_flow.take() else {
            return;
        };
        let snapshot = self.host.snapshot();
        if let CloseTarget::Document(id) = &flow.target
            && !snapshot.documents.iter().any(|doc| &doc.id == id)
        {
            return;
        }
        let project_target = !matches!(flow.target, CloseTarget::Document(_))
            && (snapshot.project.path.is_some() || snapshot.arrangement.dirty);
        let dirty_doc = snapshot.documents.into_iter().find(|doc| {
            self.document_unsaved(doc)
                && match &flow.target {
                    CloseTarget::Document(id) => id == &doc.id,
                    _ => true,
                }
        });
        let dirty = if matches!(flow.target, CloseTarget::Document(_)) {
            dirty_doc.is_some()
        } else {
            snapshot.project.dirty
        };
        if !frozen && flow.saving && self.close_save_job.is_none() && self.project_picker.is_none()
        {
            if !flow.discard && dirty {
                if flow.save_authorized {
                    if project_target {
                        self.project_save(ctx, false, true);
                    } else if let Some(doc) = &dirty_doc {
                        self.file_action(FileAction::Save, &doc.id, true);
                    }
                }
                if !flow.save_authorized
                    || (self.output_dialog.is_none()
                        && self.close_save_job.is_none()
                        && self.project_picker.is_none())
                {
                    flow.saving = false;
                }
            } else {
                if flow.discard && !flow.cancelled_jobs {
                    for job in self
                        .host
                        .snapshot()
                        .jobs
                        .into_iter()
                        .filter(|job| job.status == JobStatus::Running)
                    {
                        self.host.execute(Request::new(
                            "close-cancel",
                            Command::CancelJob(JobParams { job_id: job.id }),
                        ));
                    }
                    flow.cancelled_jobs = true;
                }
                let command = match &flow.target {
                    CloseTarget::Document(id) => Command::CloseDocument(CloseParams {
                        document_id: id.clone(),
                        discard_changes: flow.discard,
                    }),
                    CloseTarget::Quit => Command::RequestQuit(QuitParams {
                        discard_changes: flow.discard,
                        expected_presentation_revision: None,
                    }),
                    CloseTarget::OpenProject(path) => {
                        Command::OpenProject(wavebob::projects::OpenProjectParams {
                            path: path.clone(),
                            discard_changes: flow.discard,
                        })
                    }
                };
                match self
                    .host
                    .execute(Request::new("gui-close", command))
                    .outcome
                {
                    Outcome::Ok { .. } => return,
                    Outcome::Error { error } if error.code == wavebob::ErrorCode::Busy => {}
                    Outcome::Error { error } => {
                        self.error = Some(error.message);
                        flow.saving = false;
                    }
                }
            }
        }
        if self.output_dialog.is_some() {
            self.close_flow = Some(flow);
            self.output_modal(ctx, help);
            return;
        }
        let mut cancel = false;
        let response = egui::Modal::new(egui::Id::unique("dirty-close-dialog")).show(ctx, |ui| {
            ui.set_max_width(430.0);
            ui.heading(if flow.saving { "Finishing work…" } else if project_target { "Unsaved project" } else { "Unsaved audio" });
            if !flow.saving {
                ui.label(if matches!(flow.target, CloseTarget::OpenProject(_)) { "Save changes before opening another project?" } else { "Save changes before closing?" });
                if project_target { ui.label("The project stores the arrangement and lossless working audio together."); }
                if let Some(doc) = &dirty_doc { ui.label(&doc.name); }
                if let Some(error) = &self.error { ui.colored_label(Color32::LIGHT_RED, error); }
                ui.horizontal(|ui| {
                    let response = ui.button(if project_target { "Save project" } else { "Save" });
                    help.observe(&response, "Save the current work before continuing. Project assets preserve working audio losslessly.");
                    if response.clicked() && !frozen { flow.saving = true; flow.save_authorized = true; }
                    let response = ui.button("Discard");
                    help.observe(&response, "Discard unsaved workspace changes without writing them.");
                    if response.clicked() && !frozen { flow.discard = true; flow.saving = true; }
                    let response = ui.button("Cancel");
                    help.observe(&response, "Keep the current workspace open.");
                    cancel = response.clicked() && !frozen;
                });
            } else {
                ui.label("Waiting for save or output publication to finish.");
                if !matches!(flow.target, CloseTarget::Document(_)) && !flow.discard && self.project_picker.is_none() {
                    let response = ui.button("Stop jobs and close");
                    help.observe(&response, "Cancel unfinished work and continue with explicit discard intent.");
                    if response.clicked() && !frozen { flow.discard = true; flow.cancelled_jobs = false; }
                }
                let response = ui.button("Cancel close");
                help.observe(&response, "Keep editing; an already accepted save may still finish.");
                cancel = response.clicked() && !frozen;
            }
        });
        cancel |= response.should_close() && !frozen;
        if !cancel {
            self.close_flow = Some(flow);
        } else {
            self.close_save_job = None;
        }
    }
}
