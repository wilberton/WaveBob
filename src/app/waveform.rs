use super::*;

#[derive(Clone)]
pub(super) enum Gesture {
    Selection {
        document_id: String,
        audio_revision: String,
        session_revision: u64,
        anchor: u64,
        current: u64,
        view: FrameRange,
        plot: Rect,
    },
    Navigation {
        document_id: String,
        audio_revision: String,
        initial: FrameRange,
        origin: f32,
        edge: i8,
        overview: bool,
    },
}

fn frame_at(x: f32, rect: Rect, view: &FrameRange) -> u64 {
    view.start
        + (((x - rect.left()) / rect.width().max(1.0)).clamp(0.0, 1.0) as f64
            * (view.end - view.start) as f64)
            .round() as u64
}

// Return the fixed opposite boundary for the closest visible selection edge.
fn selection_edge(x: f32, plot: Rect, view: &FrameRange, selection: &FrameRange) -> Option<u64> {
    [
        (selection.start, selection.end),
        (selection.end, selection.start),
    ]
    .into_iter()
    .filter(|(edge, _)| (view.start..=view.end).contains(edge))
    .map(|(edge, anchor)| {
        let edge_x = plot.left()
            + (edge - view.start) as f32 / (view.end - view.start).max(1) as f32 * plot.width();
        ((x - edge_x).abs(), anchor)
    })
    .filter(|(distance, _)| *distance <= 7.0)
    .min_by(|a, b| a.0.total_cmp(&b.0))
    .map(|(_, anchor)| anchor)
}

impl WaveBob {
    pub(super) fn display_document(&self) -> Option<DocumentInfo> {
        let mut document = self.active_document()?;
        if let Ok(state) = self
            .host
            .processing(wavebob::processing::Owner::Document(document.id.clone()))
        {
            document.processing = state.preview.unwrap_or(state.committed);
        }
        if let Some(Gesture::Selection {
            document_id,
            audio_revision,
            anchor,
            current,
            ..
        }) = &self.gesture
            && document.id == *document_id
            && document.revision_id == *audio_revision
        {
            let start = (*anchor).min(*current);
            let end = (*anchor).max(*current);
            document.cursor = start;
            document.selection = (start < end).then_some(FrameRange { start, end });
        }
        Some(document)
    }
    pub(super) fn cancel_gesture(&mut self) {
        if let Some(Gesture::Navigation {
            document_id,
            initial,
            ..
        }) = self.gesture.take()
        {
            self.set_viewport(&document_id, initial);
        }
    }
    fn set_viewport(&mut self, id: &str, view: FrameRange) {
        self.dispatch(Command::SetViewport(SetViewportParams {
            document_id: id.into(),
            start_frame: view.start,
            end_frame: view.end,
            expected_presentation_revision: Some(self.host.presentation().revision),
        }));
    }
    fn validate_gesture(&mut self, document: &DocumentInfo) {
        let valid = match &self.gesture {
            Some(Gesture::Selection {
                document_id,
                audio_revision,
                ..
            })
            | Some(Gesture::Navigation {
                document_id,
                audio_revision,
                ..
            }) => *document_id == document.id && *audio_revision == document.revision_id,
            None => true,
        };
        if !valid {
            self.gesture = None;
        }
    }
    fn navigate_input(
        &mut self,
        response: &egui::Response,
        document: &DocumentInfo,
        view: &FrameRange,
    ) {
        if self.host.frozen() || !response.contains_pointer() || response.ctx.text_edit_focused() {
            return;
        }
        let Some(pointer) = response.ctx.input(|input| input.pointer.hover_pos()) else {
            return;
        };
        if !response.rect.contains(pointer) {
            return;
        }
        let (scroll, factor) = response.ctx.input_mut(|input| {
            let delta = input.smooth_scroll_delta;
            input.smooth_scroll_delta = Vec2::ZERO;
            (delta, input.zoom_delta())
        });
        if (factor - 1.0).abs() > f32::EPSILON {
            let plot = visuals::main_plot(response.rect);
            let anchor = ((pointer.x - plot.left()) / plot.width()).clamp(0.0, 1.0) as f64;
            self.set_viewport(
                &document.id,
                zoom(view, factor as f64, anchor, document.frames),
            );
        } else if scroll != Vec2::ZERO {
            self.set_viewport(
                &document.id,
                pan(
                    view,
                    -(scroll.x + scroll.y) as f64 * (view.end - view.start) as f64
                        / response.rect.width() as f64,
                    document.frames,
                ),
            );
        }
    }
    fn main_gestures(
        &mut self,
        response: &egui::Response,
        document: &DocumentInfo,
        view: &FrameRange,
    ) {
        if self.host.frozen() || self.envelope_drag.is_some() || self.envelope_pointer_claimed {
            return;
        }
        let ctx = &response.ctx;
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        let plot = visuals::main_plot(response.rect);
        let pointer = ctx.input(|input| input.pointer.interact_pos());
        let origin = ctx.input(|input| input.pointer.press_origin());
        if response.hovered()
            && let Some(pointer) = pointer
            && plot.contains(pointer)
            && document
                .selection
                .as_ref()
                .is_some_and(|selection| selection_edge(pointer.x, plot, view, selection).is_some())
        {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(origin) = origin
        {
            let anchor = document
                .selection
                .as_ref()
                .filter(|_| plot.contains(origin))
                .and_then(|selection| selection_edge(origin.x, plot, view, selection))
                .unwrap_or_else(|| frame_at(origin.x, plot, view));
            self.gesture = Some(Gesture::Selection {
                document_id: document.id.clone(),
                audio_revision: document.revision_id.clone(),
                session_revision: self.host.snapshot().revision,
                anchor,
                current: anchor,
                view: view.clone(),
                plot,
            });
        }
        if response.drag_started_by(egui::PointerButton::Middle)
            && let Some(origin) = origin
        {
            self.gesture = Some(Gesture::Navigation {
                document_id: document.id.clone(),
                audio_revision: document.revision_id.clone(),
                initial: view.clone(),
                origin: origin.x,
                edge: 0,
                overview: false,
            });
        }
        if let Some(pointer) = pointer {
            match &mut self.gesture {
                Some(Gesture::Selection {
                    current,
                    view,
                    plot,
                    ..
                }) => *current = frame_at(pointer.x, *plot, view),
                Some(Gesture::Navigation {
                    initial,
                    origin,
                    overview: false,
                    ..
                }) => {
                    let next = pan(
                        initial,
                        -(pointer.x - *origin) as f64 * (initial.end - initial.start) as f64
                            / plot.width() as f64,
                        document.frames,
                    );
                    self.set_viewport(&document.id, next);
                }
                _ => {}
            }
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            if let Some(Gesture::Selection {
                document_id,
                session_revision,
                anchor,
                current,
                ..
            }) = self.gesture.take()
            {
                let request = Request::new(
                    "gui-selection",
                    Command::SetSelection(SelectionParams {
                        document_id,
                        start_frame: anchor,
                        end_frame: current,
                    }),
                )
                .expecting_revision(session_revision);
                self.error = match self.host.execute(request).outcome {
                    Outcome::Error { error } => Some(error.message),
                    _ => None,
                };
            }
        } else if response.drag_stopped_by(egui::PointerButton::Middle) {
            self.gesture = None;
        } else if response.clicked_by(egui::PointerButton::Primary)
            && let Some(pointer) = pointer
        {
            self.dispatch(Command::SetCursor(CursorParams {
                document_id: document.id.clone(),
                frame: frame_at(pointer.x, plot, view),
            }));
        }
        self.navigate_input(response, document, view);
    }
    fn overview_gestures(
        &mut self,
        response: &egui::Response,
        document: &DocumentInfo,
        view: &FrameRange,
    ) {
        if self.host.frozen() {
            return;
        }
        let plot = response.rect.shrink(1.0);
        if response.clicked() || response.drag_started() {
            response.request_focus();
        }
        let viewport = visuals::viewport_box(response.rect, document.frames, view);
        let whole = FrameRange {
            start: 0,
            end: document.frames,
        };
        let origin = response.ctx.input(|input| input.pointer.press_origin());
        if response.drag_started_by(egui::PointerButton::Primary)
            && let Some(origin) = origin
        {
            let left = (origin.x - viewport.left()).abs();
            let right = (origin.x - viewport.right()).abs();
            let edge = if left.min(right) <= 7.0 {
                if left <= right { -1 } else { 1 }
            } else {
                0
            };
            self.gesture = Some(Gesture::Navigation {
                document_id: document.id.clone(),
                audio_revision: document.revision_id.clone(),
                initial: view.clone(),
                origin: origin.x,
                edge,
                overview: true,
            });
        }
        if let Some(Gesture::Navigation {
            initial,
            origin,
            edge,
            overview: true,
            ..
        }) = &self.gesture
            && let Some(pointer) = response.interact_pointer_pos()
        {
            let frame = frame_at(pointer.x, plot, &whole);
            let next = match edge {
                -1 => FrameRange {
                    start: frame.min(initial.end - 1),
                    end: initial.end,
                },
                1 => FrameRange {
                    start: initial.start,
                    end: frame.max(initial.start + 1),
                },
                _ => pan(
                    initial,
                    (pointer.x - *origin) as f64 * document.frames as f64 / plot.width() as f64,
                    document.frames,
                ),
            };
            self.set_viewport(&document.id, next);
        }
        if response.drag_stopped_by(egui::PointerButton::Primary) {
            self.gesture = None;
        } else if response.clicked_by(egui::PointerButton::Primary)
            && let Some(pointer) = response.interact_pointer_pos()
            && !viewport.contains(pointer)
        {
            let frame = frame_at(pointer.x, plot, &whole);
            self.set_viewport(
                &document.id,
                pan(
                    view,
                    frame as f64 - (view.start as f64 + view.end as f64) / 2.0,
                    document.frames,
                ),
            );
        }
        if let Some(pointer) = response.ctx.input(|input| input.pointer.hover_pos())
            && response.hovered()
        {
            response.ctx.set_cursor_icon(
                if (pointer.x - viewport.left())
                    .abs()
                    .min((pointer.x - viewport.right()).abs())
                    <= 7.0
                {
                    egui::CursorIcon::ResizeHorizontal
                } else {
                    egui::CursorIcon::Grab
                },
            );
        }
    }

    pub(super) fn waveform_view(&mut self, ui: &mut egui::Ui, help: &mut Help) {
        let Some(document) = self.active_document() else {
            self.gesture = None;
            self.host.set_transient_resources(Vec::new());
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Open an audio file to begin")
                        .size(18.0)
                        .color(MUTED),
                );
            });
            return;
        };
        self.validate_gesture(&document);
        self.update_waveform();
        let view = self.host.presentation().visible_interval(&document);
        let (toolbar, _) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::hover());
        let mut info = child(
            ui,
            Rect::from_min_max(
                toolbar.min,
                Pos2::new(toolbar.right() - 125.0, toolbar.bottom()),
            ),
        );
        info.horizontal(|ui| {
            ui.label(RichText::new(&document.name).strong());
            ui.label(
                RichText::new(format!(
                    "{} kHz / {} / {}   |   {}",
                    document.sample_rate as f64 / 1000.0,
                    if document.channels == 1 {
                        "Mono"
                    } else {
                        "Stereo"
                    },
                    encoding_label(&document),
                    time(document.frames, document.sample_rate)
                ))
                .small()
                .color(MUTED),
            );
        });
        let mut zoom_ui = child(
            ui,
            Rect::from_min_max(
                Pos2::new(toolbar.right() - 116.0, toolbar.top()),
                toolbar.max,
            ),
        );
        zoom_ui.spacing_mut().item_spacing.x = 4.0;
        zoom_ui.horizontal(|ui| {for (icon,label,factor) in [(Icon::Minus,"Zoom out",0.5),(Icon::Plus,"Zoom in",2.0),(Icon::Fit,"Fit sample",0.0)] {
            let response=icon_button(ui,icon,label,false,document.frames>0);
            help.observe(&response,if factor==0.0 {"Show the whole sample without changing the selection."} else {"Zoom around the visible center. Ctrl/Cmd + scroll or pinch zooms around the pointer."});
            if response.clicked() {self.set_viewport(&document.id,if factor==0.0 {FrameRange{start:0,end:document.frames}} else {zoom(&view,factor,0.5,document.frames)});}
        }});
        ui.add_space(4.0);
        if document.frames == 0 {
            self.gesture = None;
            self.host.set_transient_resources(Vec::new());
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Empty sample — paste audio or undo the deletion")
                        .size(18.0)
                        .color(MUTED),
                );
            });
            return;
        }
        let preview = self.display_document().unwrap();
        if let Some(wave) = &self.waveform {
            let response = ui
                .push_id(("overview", &document.id, &document.revision_id), |ui| {
                    visuals::waveform_canvas(ui, 72.0, wave, &preview, &view, true)
                })
                .inner;
            help.observe(
                &response,
                "Drag the overview box to pan; drag an edge to zoom with the opposite edge fixed.",
            );
            self.overview_gestures(&response, &document, &view);
        }
        ui.add_space(12.0);
        if let Some(wave) = &self.detail {
            let response = ui
                .push_id(
                    ("main-waveform", &document.id, &document.revision_id),
                    |ui| {
                        visuals::waveform_canvas(
                            ui,
                            ui.available_height().max(80.0),
                            wave,
                            &preview,
                            &view,
                            false,
                        )
                    },
                )
                .inner;
            help.observe(&response,"Click to place the cursor; drag to select across channels; drag a selection edge to adjust it. Scroll or middle-drag pans; Ctrl/Cmd + scroll or pinch zooms. Escape cancels.");
            let plot = visuals::main_plot(response.rect);
            let body = Rect::from_min_max(
                Pos2::new(
                    plot.left()
                        - (view.start as f64 / (view.end - view.start) as f64 * plot.width() as f64)
                            as f32,
                    plot.top(),
                ),
                Pos2::new(
                    plot.left()
                        + ((document.frames - view.start) as f64 / (view.end - view.start) as f64
                            * plot.width() as f64) as f32,
                    plot.bottom(),
                ),
            );
            let owner = wavebob::processing::Owner::Document(document.id.clone());
            self.envelope_handles(
                ui,
                &envelopes::Target {
                    owner: owner.clone(),
                    body,
                    visible: plot,
                    frames: document.frames,
                    rate: document.sample_rate,
                    reserved: Vec::new(),
                    selected: true,
                },
                help,
            );
            response.context_menu(|ui| {
                if ui.button("Gain and fades…").clicked() {
                    self.open_envelope_entry(owner);
                    ui.close();
                }
            });
            self.main_gestures(&response, &document, &view);
        }
        self.host
            .set_transient_resources(if self.gesture.is_some() {
                vec!["waveform-gesture-preview".into()]
            } else {
                Vec::new()
            });
    }
}
