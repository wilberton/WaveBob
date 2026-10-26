use super::*;
use wavebob::browser::{AuditionVolumeParams, PathParams};

impl WaveBob {
    pub(super) fn browser_view(&mut self, ui: &mut egui::Ui, area: Rect, help: &mut Help) {
        let state = self.host.browser();
        if !self.host.frozen() && state.path != self.browser_seen_path {
            if let Some(path) = &state.path {
                self.browser_path = path.clone();
            }
            self.browser_seen_path = state.path.clone();
            self.browser_edit_path = false;
        }
        let footer_height = 112.0_f32.min(area.height() * 0.4);
        let mut browser = child(
            ui,
            Rect::from_min_max(
                area.min,
                Pos2::new(area.right(), area.bottom() - footer_height),
            ),
        );
        section(&mut browser, "FILE BROWSER");
        browser.add_space(5.0);
        browser.horizontal(|ui| {
            let parent =
                ui.add_enabled(state.parent.is_some(), egui::Button::new("↑").frame(false));
            help.observe(&parent, "Go to the parent folder.");
            if parent.clicked()
                && let Some(path) = state.parent.clone()
            {
                self.browser_path = path.clone();
                self.dispatch(Command::BrowseFolder(PathParams { path }));
            }
            if !self.browser_edit_path {
                let label = state
                    .path
                    .as_deref()
                    .and_then(|path| std::path::Path::new(path).file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or("Choose folder…".into());
                let response = ui.add_sized([ui.available_width(), 25.0], centered_button(&label));
                help.observe(
                    &response,
                    &format!(
                        "{} — click to enter a folder path.",
                        state.path.as_deref().unwrap_or("No folder selected")
                    ),
                );
                if response.clicked() {
                    self.browser_edit_path = true;
                }
            } else {
                let response = ui.add_sized(
                    [ui.available_width() - 32.0, 25.0],
                    egui::TextEdit::singleline(&mut self.browser_path).hint_text("Folder path"),
                );
                help.observe(
                    &response,
                    "Enter a folder path and press Go to browse audio files.",
                );
                let go = ui.small_button("Go");
                help.observe(&go, "List this folder without blocking the editor.");
                if go.clicked() {
                    self.dispatch(Command::BrowseFolder(PathParams {
                        path: self.browser_path.clone(),
                    }));
                }
            }
        });
        if state.loading {
            browser.label(RichText::new("Reading folder…").small().color(MUTED));
        }
        if let Some(error) = &state.error {
            browser.colored_label(Color32::LIGHT_RED, &error.message);
        }
        if state.skipped_paths > 0 {
            browser.label(
                RichText::new(format!("{} unreadable paths skipped", state.skipped_paths))
                    .small()
                    .color(MUTED),
            );
        }
        if state.path.is_none() && !state.loading {
            browser.label(
                RichText::new("Choose a folder to browse")
                    .small()
                    .color(MUTED),
            );
        }
        browser.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt(("file-browser",&state.path)).auto_shrink([false,false]).show_rows(&mut browser,24.0,state.entries.len(),|ui,rows|{
            for entry in &state.entries[rows] {
                let response=visuals::file_row(ui,&entry.name,state.selected_path.as_ref()==Some(&entry.path),entry.directory,entry.directory||entry.supported);
                let description=if entry.directory{"Double-click to open this folder."} else if entry.supported{"Click to select. Audition previews without opening; double-click opens for editing."} else{"Unsupported file type. WaveBob opens WAV, MP3 and Ogg Vorbis audio."};
                help.observe(&response,description);
                if response.double_clicked() {
                    if entry.directory {self.browser_path=entry.path.clone();self.dispatch(Command::BrowseFolder(PathParams{path:entry.path.clone()}));}
                    else if entry.supported {self.dispatch(Command::OpenBrowserFile(PathParams{path:entry.path.clone()}));self.set_mode(ViewMode::Waveform);}
                } else if response.clicked()&&!entry.directory {self.dispatch(Command::SelectBrowserFile(PathParams{path:entry.path.clone()}));}
            }
        });
        let mut preview = child(
            ui,
            Rect::from_min_max(
                Pos2::new(area.left(), area.bottom() - footer_height + 4.0),
                area.max,
            ),
        );
        preview.separator();
        preview.label(RichText::new("Browser preview").strong().small());
        let name = state
            .selected_path
            .as_deref()
            .and_then(|path| std::path::Path::new(path).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or("No file selected".into());
        preview.add(egui::Label::new(RichText::new(name).small().color(MUTED)).truncate());
        let audition = &state.audition;
        if let Some(error) = &audition.error {
            preview.add(
                egui::Label::new(
                    RichText::new(&error.message)
                        .small()
                        .color(Color32::LIGHT_RED),
                )
                .truncate(),
            );
        } else if audition.loading {
            preview.label(RichText::new("Preparing preview…").small().color(MUTED));
        }
        preview.horizontal(|ui|{
            let active=audition.loading||audition.playing;
            let supported=state.selected_path.as_deref().is_some_and(|path|std::path::Path::new(path).extension().is_some_and(|ext|ext.eq_ignore_ascii_case("wav")||ext.eq_ignore_ascii_case("mp3")));
            let enabled=active||(supported&&self.host.transport().output.status==wavebob::transport::OutputStatus::Ready);
            let response=icon_button(ui,if active{Icon::Pause}else{Icon::Play},"Audition",active,enabled);
            help.observe(&response,if active{"Stop preview without resuming editor playback."}else if !supported{"Select a WAV, MP3 or Ogg Vorbis file to enable audition."}else if !enabled{"Audition needs an available audio output. Use Retry output if the device failed."}else{"Audition the selected WAV, MP3 or Ogg Vorbis without creating a document. Editor playback pauses."});
            if response.clicked(){if active{self.dispatch(Command::StopAudition(EmptyParams{}));}else if let Some(path)=state.selected_path.clone(){self.dispatch(Command::StartAudition(PathParams{path}));}}
            let mut volume=state.audition_volume;
            ui.spacing_mut().slider_width=(area.width()-65.0).max(30.0);
            let response=ui.add(egui::Slider::new(&mut volume,0.0..=1.0).show_value(false));help.observe(&response,"Independent browser preview volume.");
            if response.changed(){self.dispatch(Command::SetAuditionVolume(AuditionVolumeParams{volume}));}
        });
    }
}
