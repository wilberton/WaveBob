use eframe::egui::{self, Align, Color32, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use wavebob::{Command, EmptyParams, Outcome, Request, desktop::DesktopHost, presentation::*};

const BLUE: Color32 = Color32::from_rgb(0, 122, 255);
const SURFACE: Color32 = Color32::from_rgb(20, 26, 33);
const BORDER: Color32 = Color32::from_rgb(49, 60, 73);
const MUTED: Color32 = Color32::from_rgb(145, 160, 179);
const FILES: [&str; 3] = ["Rain ambience.wav", "Footsteps.wav", "Piano texture.wav"];

#[derive(Default)]
pub struct WaveBob {
    selected: usize,
    host: DesktopHost,
    error: Option<String>,
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
    fn dispatch(&mut self, command: Command) {
        if self.host.frozen() {
            return;
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
        Ok(app)
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.text_edit_focused()
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
        let toggle = pressed(egui::Key::Backtick);
        if toggle {
            self.dispatch(Command::TogglePanels(PresentationRevisionParams::default()));
        }
        if pressed(egui::Key::Slash) {
            self.toggle_loop();
        }
    }

    // Independent of eframe::Frame for real egui pointer/key/focus tests.
    fn draw(&mut self, ui: &mut egui::Ui) {
        self.host.begin_frame(ui.ctx());
        self.shortcuts(ui.ctx());
        let presentation = self.host.presentation();
        let mode = presentation.mode;
        let mut help = Help::default();
        let error = self.error.clone().or_else(|| match &presentation.preferences {
            PreferenceStatus::Error { .. } => Some("Layout preferences could not be saved or loaded. Check the preferences file and its permissions.".into()),
            _ => None,
        });
        egui::Panel::top("header")
            .exact_size(42.0)
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
                    for name in ["File", "Edit", "View", "Help"] {
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
            .exact_size(if error.is_some() { 54.0 } else { 30.0 })
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
                    self.layout(SetLayoutParams { left_width: Some((pointer.x - rect.left()).clamp(MIN_PANEL_WIDTH, (full_width - MIN_CENTER_WIDTH - other).max(MIN_PANEL_WIDTH))), ..Default::default() });
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
                    self.layout(SetLayoutParams { right_width: Some((rect.right()-pointer.x).clamp(MIN_PANEL_WIDTH, (full_width - MIN_CENTER_WIDTH - other).max(MIN_PANEL_WIDTH))), ..Default::default() });
                }
            } else {
                let response = ui.put(Rect::from_min_size(rect.min+Vec2::new(1.0, 10.0), Vec2::new(22.0, 26.0)), egui::Button::new("‹").frame(false));
                help.observe(&response, "Restore the effects panel at its previous width. Backtick toggles both panels.");
                if response.clicked() { self.layout(SetLayoutParams { right_visible: Some(true), ..Default::default() }); }
            }
        });
        egui::Panel::bottom("transport").exact_size(56.0).frame(panel_frame()).show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(((ui.available_width()-300.0)/2.0).max(0.0));
                help.observe(&ui.add_enabled(false, egui::Button::new("|◀").min_size(Vec2::new(34.0,28.0))), "Rewind is unavailable until audio playback is implemented. Shortcut: Enter or comma.");
                help.observe(&ui.add_enabled(false, egui::Button::new("▶").min_size(Vec2::new(44.0,28.0))), "Playback is unavailable: no audio document is open. Shortcut: Space.");
                let response = ui.add(egui::Button::new("Loop").selected(self.host.snapshot().loop_enabled).min_size(Vec2::new(44.0,28.0)));
                help.observe(&response, "Toggle looping without starting playback. Shortcut: /.");
                if response.clicked() { self.toggle_loop(); }
                ui.add_space(12.0);
                ui.label(RichText::new("—  /  —").monospace().color(MUTED));
            });
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
        self.status(ui, status, mode, &help, error.as_deref());
        self.host.end_frame(ui.ctx());
    }

    fn library(&mut self, ui: &mut egui::Ui, area: Rect, help: &mut Help, layout: &PanelLayout) {
        let split_y = area.top() + area.height() * layout.library_fraction;
        let upper = Rect::from_min_max(area.min, Pos2::new(area.right(), split_y - 4.0));
        let lower = Rect::from_min_max(Pos2::new(area.left(), split_y + 4.0), area.max);
        let mut library = child(ui, upper);
        library.horizontal(|ui| {
            section(ui, "OPEN FILES");
            let response = ui.put(
                Rect::from_min_size(
                    Pos2::new(upper.right() - 22.0, upper.top()),
                    Vec2::new(22.0, 24.0),
                ),
                egui::Button::new("‹").frame(false),
            );
            help.observe(
                &response,
                "Hide the file panel. Backtick toggles both panels.",
            );
            if response.clicked() {
                self.layout(SetLayoutParams {
                    left_visible: Some(false),
                    ..Default::default()
                });
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("open-files")
            .show(&mut library, |ui| {
                ui.label(
                    RichText::new("No audio documents open")
                        .small()
                        .color(MUTED),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("SYNTHETIC PREVIEWS").size(10.0).color(MUTED));
                for (index, name) in FILES.iter().enumerate() {
                    let response = ui.selectable_label(self.selected == index, *name);
                    help.observe(
                        &response,
                        "Show a synthetic waveform preview. This is not an open audio document.",
                    );
                    if response.clicked() && !self.host.frozen() {
                        self.selected = index;
                        self.set_mode(ViewMode::Waveform);
                    }
                }
            });
        let handle = Rect::from_min_max(
            Pos2::new(area.left(), split_y - 4.0),
            Pos2::new(area.right(), split_y + 4.0),
        );
        let response = ui
            .interact(handle, ui.scope_id().with("library-split"), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
        ui.painter().line_segment(
            [handle.left_center(), handle.right_center()],
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
                    ((pointer.y - area.top()) / area.height()).clamp(0.15, 0.85),
                ),
                ..Default::default()
            });
        }
        let mut browser = child(ui, lower);
        section(&mut browser, "FILE BROWSER");
        browser.add_space(6.0);
        browser.label(
            RichText::new("Samples  ›  Field recordings")
                .small()
                .color(MUTED),
        );
        browser.separator();
        for folder in ["City", "Forest", "Indoor", "Weather"] {
            help.observe(
                &browser.add_enabled(
                    false,
                    egui::Button::new(format!("▸  {folder}")).frame(false),
                ),
                "Filesystem navigation is not implemented yet.",
            );
        }
        browser.add_space(10.0);
        for name in [
            "Birds in trees.wav",
            "Creek close.wav",
            "Rain ambience.wav",
            "Wind gusts.wav",
        ] {
            help.observe(
                &browser.add_enabled(false, egui::Button::new(name).frame(false)),
                "Opening and auditioning filesystem audio are not implemented yet.",
            );
        }
        browser.add_space(12.0);
        help.observe(
            &browser.add_enabled(false, egui::Button::new("▶  Audition")),
            "Audition is unavailable until filesystem browsing and audio playback are implemented.",
        );
    }

    fn effects(&mut self, ui: &mut egui::Ui, area: Rect, help: &mut Help) {
        let mut content = child(ui, area);
        content.horizontal(|ui| {
            let response = ui.button("›");
            help.observe(
                &response,
                "Hide the effects panel. Backtick toggles both panels.",
            );
            if response.clicked() {
                self.layout(SetLayoutParams {
                    right_visible: Some(false),
                    ..Default::default()
                });
            }
            section(ui, "EDIT & EFFECTS");
        });
        content.add_space(16.0);
        content.label("Target");
        help.observe(
            &content.add_enabled(
                false,
                egui::Button::new("No editing target  ▾").min_size(Vec2::new(area.width(), 26.0)),
            ),
            "Effects need an audio document or a selected arrangement clip.",
        );
        content.add_space(16.0);
        content.horizontal(|ui| {
            ui.label("Gain");
            ui.label(RichText::new("— dB").color(MUTED));
        });
        let mut gain = 0.0;
        help.observe(
            &content.add_enabled(
                false,
                egui::Slider::new(&mut gain, -24.0..=12.0).show_value(false),
            ),
            "Gain is unavailable until an editable audio target is open.",
        );
        content.add_space(10.0);
        for pair in [["Fade in", "Fade out"], ["Reverse", "Reverb"]] {
            content.horizontal(|ui| {
                for label in pair {
                    help.observe(
                        &ui.add_enabled(
                            false,
                            egui::Button::new(label)
                                .min_size(Vec2::new((area.width() - 8.0) / 2.0, 28.0)),
                        ),
                        "This audio effect is not implemented yet; no editing target is open.",
                    );
                }
            });
        }
        content.add_space(6.0);
        help.observe(
            &content.add_enabled(
                false,
                egui::Button::new("Apply")
                    .fill(BLUE)
                    .min_size(Vec2::new(area.width(), 30.0)),
            ),
            "Apply is unavailable: no audio document or selected clip can be edited yet.",
        );
    }

    fn waveform_view(&self, ui: &mut egui::Ui, help: &mut Help) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(FILES[self.selected]).strong());
            ui.label(
                RichText::new("SYNTHETIC PREVIEW · No audio loaded")
                    .small()
                    .color(MUTED),
            );
        });
        ui.add_space(10.0);
        help.observe(
            &waveform(ui, 70.0, self.selected, true, BLUE),
            "Waveform overview preview. Panning and zooming become available with real audio.",
        );
        ui.add_space(12.0);
        ui.label(RichText::new("WAVEFORM PREVIEW").size(10.0).color(MUTED));
        let height = ((ui.available_height() - 10.0) / 2.0).max(40.0);
        for channel in 0..2 {
            help.observe(&waveform(ui,height,self.selected+channel,false,BLUE),"Synthetic waveform preview. Cursor and selection are empty until an audio document is open.");
        }
    }

    fn multitrack_view(&self, ui: &mut egui::Ui, help: &mut Help) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Arrangement").strong());
            ui.label(
                RichText::new("SYNTHETIC PREVIEW · No clips added")
                    .small()
                    .color(MUTED),
            );
        });
        ui.add_space(10.0);
        egui::ScrollArea::vertical().show(ui,|ui| {
            for (index,(name,color)) in [("Ambience",Color32::from_rgb(69,166,175)),("Footsteps",BLUE),("Texture",Color32::from_rgb(201,163,67)),("Track 4",MUTED)].iter().enumerate() {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(Vec2::new(150.0,100.0),egui::Layout::top_down(Align::Min),|ui| {
                        ui.label(RichText::new(format!("Track {}",index+1)).small().color(MUTED));
                        ui.label(RichText::new(*name).strong());
                        ui.horizontal(|ui| {
                            help.observe(&ui.add_enabled(false,egui::Button::new("M")),"Track mute is unavailable in this synthetic arrangement preview.");
                            help.observe(&ui.add_enabled(false,egui::Button::new("S")),"Track solo is unavailable in this synthetic arrangement preview.");
                            ui.label(RichText::new("0 dB · C").small().color(MUTED));
                        });
                    });
                    if index<3 { help.observe(&waveform(ui,90.0,index,false,*color),"Arrangement preview. Clip placement and track editing are not implemented yet."); }
                    else { help.observe(&ui.label("No clips · Arrangement editing is planned"),"Add real audio and tracks in the upcoming arrangement milestone."); }
                });
            }
        });
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
        content.horizontal(|ui| {
            let readout = if mode == ViewMode::Waveform {
                "No document · Cursor — · No selection"
            } else {
                "Empty arrangement · Selection: none · Selected clips: 0"
            };
            ui.label(RichText::new(readout).size(11.0));
            ui.separator();
            let saving = matches!(
                self.host.presentation().preferences,
                PreferenceStatus::Saving
            );
            let available = (ui.available_width() - if saving { 95.0 } else { 0.0 }).max(0.0);
            ui.add_sized(
                [available, 18.0],
                egui::Label::new(RichText::new(help.text()).size(11.0).color(MUTED)).truncate(),
            );
            if saving {
                ui.label(RichText::new("Saving layout…").size(10.0).color(MUTED));
            }
        });
    }
}

impl eframe::App for WaveBob {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(SURFACE)
        .stroke(Stroke::new(1.0, BORDER))
}
fn section(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).size(11.0).strong());
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
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = SURFACE;
    visuals.window_fill = SURFACE;
    visuals.override_text_color = Some(Color32::from_rgb(209, 220, 235));
    visuals.selection.bg_fill = BLUE;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(28, 36, 45);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
    ctx.set_visuals(visuals);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = Vec2::new(7.0, 6.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        style.explanation_tooltips = false;
        style.url_in_tooltip = false;
        style
            .text_styles
            .insert(egui::TextStyle::Body, egui::FontId::proportional(13.0));
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(12.0));
    });
}

fn waveform(
    ui: &mut egui::Ui,
    height: f32,
    seed: usize,
    overview: bool,
    color: Color32,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, Color32::from_rgb(13, 19, 26));
    let middle = rect.center().y;
    painter.line_segment(
        [
            Pos2::new(rect.left(), middle),
            Pos2::new(rect.right(), middle),
        ],
        Stroke::new(1.0, BORDER),
    );
    for column in 0..(rect.width() as usize / 3) {
        let x = rect.left() + column as f32 * 3.0;
        let phase = column as f32 * 0.31 + seed as f32 * 2.0;
        let amplitude = (phase.sin() * (phase * 0.17).cos()).abs() * height * 0.4 + 2.0;
        painter.line_segment(
            [
                Pos2::new(x, middle - amplitude),
                Pos2::new(x, middle + amplitude),
            ],
            Stroke::new(1.0, color),
        );
    }
    let selection = Rect::from_min_max(
        Pos2::new(rect.left() + rect.width() * 0.25, rect.top()),
        Pos2::new(rect.left() + rect.width() * 0.6, rect.bottom()),
    );
    painter.rect_filled(
        selection,
        0.0,
        Color32::from_rgba_unmultiplied(0, 122, 255, 35),
    );
    if overview {
        for x in [selection.left(), selection.right()] {
            painter.line_segment(
                [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
                Stroke::new(2.0, BLUE),
            );
        }
    }
    response
}

#[cfg(test)]
mod tests;
