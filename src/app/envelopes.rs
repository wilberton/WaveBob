//! Shared document/clip envelope gestures; core commands own sound and history.
use super::*;
use wavebob::processing::{self, Envelope, Owner};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Handle {
    FadeIn,
    Gain,
    FadeOut,
}
#[derive(Clone)]
pub(super) struct Target {
    pub owner: Owner,
    pub body: Rect,
    pub visible: Rect,
    pub frames: u64,
    pub rate: u32,
    pub reserved: Vec<Rect>,
    pub selected: bool,
}
pub(super) struct Drag {
    owner: Owner,
    id: String,
    handle: Handle,
    current: Envelope,
    body: Rect,
    frames: u64,
}
pub(super) struct PendingClick {
    owner: Owner,
    until: f64,
}
pub(super) struct Entry {
    owner: Owner,
    revision: String,
    frames: u64,
    rate: u32,
    draft: Envelope,
    error: Option<String>,
}
fn handle_id(owner: &Owner, kind: Handle) -> egui::Id {
    egui::Id::unique(("envelope", owner, kind))
}
fn value_text(kind: Handle, envelope: &Envelope, rate: u32) -> String {
    match kind {
        Handle::Gain => envelope.gain_db.map_or_else(
            || "Gain: −∞ dB (mute)".into(),
            |value| format!("Gain: {value:+.2} dB"),
        ),
        _ => {
            let frames = if kind == Handle::FadeIn {
                envelope.fade_in_frames
            } else {
                envelope.fade_out_frames
            };
            let seconds = frames as f64 / rate as f64;
            format!(
                "{}: {} · {frames} frames",
                if kind == Handle::FadeIn {
                    "Fade in"
                } else {
                    "Fade out"
                },
                if seconds < 1.0 {
                    format!("{:.1} ms", seconds * 1000.0)
                } else {
                    format!("{seconds:.3} s")
                }
            )
        }
    }
}
fn reset(envelope: &mut Envelope, kind: Handle) {
    match kind {
        Handle::Gain => envelope.gain_db = Some(0.0),
        Handle::FadeIn => envelope.fade_in_frames = 0,
        Handle::FadeOut => envelope.fade_out_frames = 0,
    }
}
fn gain_y(target: &Target, envelope: &Envelope) -> f32 {
    let offset = envelope
        .gain_db
        .map_or(target.body.height() as f64, |gain| (-gain * 4.0).max(0.0));
    (target.body.top() + 6.0 + offset as f32)
        .min(target.body.bottom() - 6.0)
        .max(target.body.top() + 6.0)
}
pub(super) fn positions(target: &Target, envelope: &Envelope) -> [(Handle, Pos2); 3] {
    let x = |frame: u64| {
        target.body.left()
            + (frame as f64 / target.frames.max(1) as f64 * target.body.width() as f64) as f32
    };
    let y = gain_y(target, envelope);
    [
        (Handle::FadeIn, Pos2::new(x(envelope.fade_in_frames), y)),
        (Handle::Gain, Pos2::new(target.body.center().x, y)),
        (
            Handle::FadeOut,
            Pos2::new(x(target.frames.saturating_sub(envelope.fade_out_frames)), y),
        ),
    ]
}
fn hit_rect(point: Pos2, visible: Rect) -> Rect {
    let left = (point.x - 6.0).clamp(visible.left(), (visible.right() - 12.0).max(visible.left()));
    Rect::from_min_size(Pos2::new(left, point.y - 6.0), Vec2::splat(12.0)).intersect(visible)
}
impl WaveBob {
    pub(super) fn envelope_state(&self, owner: &Owner) -> Option<Envelope> {
        let state = self.host.processing(owner.clone()).ok()?;
        Some(state.preview.unwrap_or(state.committed).envelope)
    }
    pub(super) fn cancel_envelope_drag(&mut self) {
        if let Some(drag) = self.envelope_drag.take()
            && self
                .host
                .processing(drag.owner.clone())
                .is_ok_and(|state| state.preview_id.as_ref() == Some(&drag.id))
        {
            self.host.execute(Request::new(
                "cancel-envelope",
                Command::CancelProcessingPreview(processing::PreviewParams {
                    preview_id: drag.id,
                }),
            ));
        }
        self.envelope_click = None;
    }
    pub(super) fn open_envelope_entry(&mut self, owner: Owner) {
        if self.host.frozen() {
            return;
        }
        let Ok(state) = self.host.processing(owner.clone()) else {
            return;
        };
        let snapshot = self.host.snapshot();
        let rate = match &owner {
            Owner::Document(id) => snapshot
                .documents
                .iter()
                .find(|document| &document.id == id)
                .map(|document| document.sample_rate),
            Owner::Clip(id) => snapshot
                .arrangement
                .clip(id)
                .map(|(_, clip)| clip.source_rate),
        };
        let Some(rate) = rate else {
            return;
        };
        self.cancel_envelope_drag();
        self.envelope_dialog = Some(Entry {
            owner,
            revision: state.committed.edit_revision_id,
            frames: state.body_frames,
            rate,
            draft: state.committed.envelope,
            error: None,
        });
    }
    fn envelope_edit(&mut self, owner: &Owner, envelope: Envelope) {
        self.processing_action(owner, processing::Action::SetEnvelope { envelope });
    }
    pub(super) fn envelope_keyboard_owns_input(&self, ctx: &egui::Context) -> bool {
        self.envelope_focus.as_ref().is_some_and(|(owner, kind)| {
            ctx.memory(|memory| memory.has_focus(handle_id(owner, *kind)))
                && ctx.input(|input| {
                    input.key_pressed(egui::Key::Enter)
                        || input.key_pressed(egui::Key::Delete)
                        || input.key_pressed(egui::Key::Backspace)
                })
        })
    }
    pub(super) fn envelope_shortcuts(&mut self, ctx: &egui::Context) {
        if self.host.frozen() {
            return;
        }
        if self.envelope_drag.is_some()
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.cancel_envelope_drag();
            self.envelope_pointer_claimed = true;
            return;
        }
        let Some((owner, kind)) = self.envelope_focus.clone() else {
            return;
        };
        if !ctx.memory(|memory| memory.has_focus(handle_id(&owner, kind)))
            || ctx.text_edit_focused()
        {
            return;
        }
        if ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            self.open_envelope_entry(owner);
            return;
        }
        if ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)
        }) && let Some(mut envelope) = self.envelope_state(&owner)
        {
            reset(&mut envelope, kind);
            self.envelope_edit(&owner, envelope);
        }
    }
    pub(super) fn envelope_targets(
        &mut self,
        ui: &mut egui::Ui,
        mut targets: Vec<Target>,
        help: &mut Help,
    ) {
        let geometry: Vec<_> = targets
            .iter()
            .map(|target| {
                let rectangles = self
                    .envelope_state(&target.owner)
                    .map(|envelope| {
                        positions(target, &envelope)
                            .into_iter()
                            .map(|(_, point)| hit_rect(point, target.visible))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                (target.owner.clone(), target.selected, rectangles)
            })
            .collect();
        for target in &mut targets {
            for (owner, selected, rectangles) in &geometry {
                if *owner != target.owner && (!target.selected || *selected) {
                    target.reserved.extend(rectangles.iter().copied());
                }
            }
        }
        targets.sort_by_key(|target| target.selected);
        for target in targets {
            self.envelope_handles(ui, &target, help);
        }
    }
    pub(super) fn claim_envelope_pointer(&mut self, ui: &egui::Ui, target: &Target) {
        if self
            .envelope_drag
            .as_ref()
            .is_some_and(|drag| drag.owner == target.owner)
        {
            self.envelope_pointer_claimed = true;
            return;
        }
        if self.gesture.is_some()
            || self.arrangement_gesture.is_some()
            || !target.visible.is_positive()
            || target.visible.width() < 12.0
        {
            return;
        }
        let Some(envelope) = self.envelope_state(&target.owner) else {
            return;
        };
        let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) else {
            return;
        };
        let positions = positions(target, &envelope);
        self.envelope_pointer_claimed |= positions.iter().any(|(kind, point)| {
            target.visible.x_range().contains(point.x)
                && !positions
                    .iter()
                    .any(|(other, p)| other != kind && (p.x - point.x).abs() < 14.0)
                && !target
                    .reserved
                    .iter()
                    .any(|reserved| reserved.intersects(hit_rect(*point, target.visible)))
                && hit_rect(*point, target.visible).contains(pointer)
        });
    }
    pub(super) fn envelope_handles(&mut self, ui: &mut egui::Ui, target: &Target, help: &mut Help) {
        if !target.visible.is_positive() || target.frames == 0 {
            return;
        }
        let Some(envelope) = self.envelope_state(&target.owner) else {
            return;
        };
        if let Some(drag) = &self.envelope_drag
            && drag.owner == target.owner
            && self
                .host
                .processing(target.owner.clone())
                .is_ok_and(|state| state.preview_id.as_ref() != Some(&drag.id))
        {
            self.envelope_drag = None;
        }
        let painter = ui.painter_at(target.visible);
        let plateau = gain_y(target, &envelope);
        painter.hline(
            target.visible.x_range(),
            plateau,
            Stroke::new(0.5, Color32::from_rgba_unmultiplied(145, 197, 255, 90)),
        );
        let mut fades = envelope.clone();
        fades.gain_db = Some(0.0);
        let points: Vec<_> = (0..=96)
            .map(|index| {
                let x = target.visible.left() + target.visible.width() * index as f32 / 96.0;
                let frame = (((x - target.body.left()) / target.body.width().max(0.001)) as f64
                    * target.frames as f64)
                    .round()
                    .clamp(0.0, target.frames.saturating_sub(1) as f64)
                    as u64;
                let level = if envelope.gain_db.is_none() {
                    0.0
                } else {
                    fades.amplitude(frame, target.frames).clamp(0.0, 1.0) as f32
                };
                Pos2::new(
                    x,
                    target.body.bottom()
                        - 4.0
                        - level * (target.body.bottom() - 4.0 - plateau).max(0.0),
                )
            })
            .collect();
        painter.add(egui::Shape::line(
            points,
            Stroke::new(1.0, Color32::from_rgb(145, 197, 255)),
        ));
        if envelope
            .fade_in_frames
            .saturating_add(envelope.fade_out_frames)
            > target.frames
        {
            let x = |frame: u64| {
                target.body.left()
                    + (frame as f64 / target.frames.max(1) as f64 * target.body.width() as f64)
                        as f32
            };
            for points in [
                vec![
                    Pos2::new(target.body.left(), target.body.bottom() - 4.0),
                    Pos2::new(x(envelope.fade_in_frames), plateau),
                    Pos2::new(target.body.right(), plateau),
                ],
                vec![
                    Pos2::new(target.body.left(), plateau),
                    Pos2::new(
                        x(target.frames.saturating_sub(envelope.fade_out_frames)),
                        plateau,
                    ),
                    Pos2::new(target.body.right(), target.body.bottom() - 4.0),
                ],
            ] {
                painter.add(egui::Shape::line(
                    points,
                    Stroke::new(0.6, Color32::from_rgba_unmultiplied(145, 197, 255, 110)),
                ));
            }
            painter.text(
                Pos2::new(target.visible.right() - 6.0, target.visible.top() + 31.0),
                egui::Align2::RIGHT_TOP,
                "Overlapping fades multiply",
                egui::FontId::proportional(10.0),
                MUTED,
            );
        }
        let positions = positions(target, &envelope);
        let frozen = self.host.frozen();
        let other_gesture = self.envelope_drag.is_none()
            && (self.gesture.is_some() || self.arrangement_gesture.is_some());
        for (kind, point) in positions {
            let active = self
                .envelope_drag
                .as_ref()
                .is_some_and(|drag| drag.owner == target.owner && drag.handle == kind);
            let collision = positions
                .iter()
                .any(|(other, p)| *other != kind && (p.x - point.x).abs() < 14.0);
            if (!active
                && (collision
                    || target
                        .reserved
                        .iter()
                        .any(|reserved| reserved.intersects(hit_rect(point, target.visible)))))
                || !target.visible.x_range().contains(point.x)
                || target.visible.width() < 12.0
                || target.frames == 0
            {
                continue;
            }
            let rect = hit_rect(point, target.visible);
            if rect.width() < 11.9 || rect.height() < 11.9 {
                continue;
            }
            let response = ui.interact(
                rect,
                handle_id(&target.owner, kind),
                if other_gesture {
                    Sense::hover()
                } else {
                    Sense::click_and_drag()
                },
            );
            let color = if response.hovered() || response.has_focus() || active {
                Color32::WHITE
            } else {
                Color32::from_rgb(170, 212, 255)
            };
            match kind {
                Handle::Gain => {
                    painter.rect_filled(
                        Rect::from_center_size(point, Vec2::splat(7.0)),
                        0.0,
                        color,
                    );
                }
                Handle::FadeIn => {
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            point + Vec2::new(-4.0, -3.0),
                            point + Vec2::new(5.0, -3.0),
                            point + Vec2::new(0.0, 5.0),
                        ],
                        color,
                        Stroke::NONE,
                    ));
                }
                Handle::FadeOut => {
                    painter.add(egui::Shape::convex_polygon(
                        vec![
                            point + Vec2::new(-5.0, -3.0),
                            point + Vec2::new(4.0, -3.0),
                            point + Vec2::new(0.0, 5.0),
                        ],
                        color,
                        Stroke::NONE,
                    ));
                }
            }
            if (!other_gesture && response.contains_pointer()) || active {
                self.envelope_pointer_claimed = true;
            }
            let text = value_text(kind, &envelope, target.rate);
            if response.hovered() || response.has_focus() || active {
                if response.hovered() || active {
                    help.hover = Some(text.clone());
                } else {
                    help.focus = Some(text.clone());
                }
                painter.text(
                    Pos2::new(
                        (point.x + 10.0)
                            .min(target.visible.right() - text.len() as f32 * 5.5)
                            .max(target.visible.left() + 3.0),
                        if point.y + 30.0 > target.visible.bottom() {
                            point.y - 22.0
                        } else {
                            point.y + 14.0
                        },
                    ),
                    egui::Align2::LEFT_TOP,
                    &text,
                    egui::FontId::proportional(11.0),
                    Color32::WHITE,
                );
                ui.ctx().set_cursor_icon(if kind == Handle::Gain {
                    egui::CursorIcon::ResizeVertical
                } else {
                    egui::CursorIcon::ResizeHorizontal
                });
            }
            if frozen || other_gesture {
                continue;
            }
            if response.clicked() || response.drag_started() {
                response.request_focus();
                self.envelope_focus = Some((target.owner.clone(), kind));
            }
            if response.double_clicked() {
                self.envelope_click = None;
                let mut next = envelope.clone();
                reset(&mut next, kind);
                self.envelope_edit(&target.owner, next);
            } else if response.clicked() {
                self.envelope_click = Some(PendingClick {
                    owner: target.owner.clone(),
                    until: ui.input(|input| input.time) + 0.35,
                });
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(360));
            }
            if response.drag_started() {
                self.envelope_click = None;
                self.gesture = None;
                self.arrangement_gesture = None;
                let Ok(state) = self.host.processing(target.owner.clone()) else {
                    continue;
                };
                let result = self.host.execute(Request::new(
                    "envelope-begin",
                    Command::BeginProcessingPreview(processing::BeginParams {
                        owner: target.owner.clone(),
                        expected_edit_revision: state.committed.edit_revision_id,
                    }),
                ));
                if let Outcome::Ok {
                    result: CommandResult::Processing(state),
                } = result.outcome
                    && let Some(id) = state.preview_id
                {
                    self.envelope_drag = Some(Drag {
                        owner: target.owner.clone(),
                        id,
                        handle: kind,
                        current: state.committed.envelope,
                        body: target.body,
                        frames: target.frames,
                    });
                }
            }
            if response.dragged()
                && let Some(drag) = &mut self.envelope_drag
                && drag.owner == target.owner
                && drag.handle == kind
            {
                let fine = ui.input(|input| if input.modifiers.shift { 0.1 } else { 1.0 });
                match kind {
                    Handle::Gain => {
                        drag.current.gain_db = Some(
                            (drag.current.gain_db.unwrap_or(-60.0)
                                - ui.input(|input| input.pointer.delta().y) as f64 * 0.25 * fine)
                                .clamp(-60.0, 12.0),
                        )
                    }
                    _ => {
                        if let Some(pointer) = response.interact_pointer_pos() {
                            let local = (((pointer.x - drag.body.left())
                                / drag.body.width().max(0.001))
                                as f64
                                * drag.frames as f64)
                                .round()
                                .clamp(0.0, drag.frames as f64)
                                as u64;
                            if kind == Handle::FadeIn {
                                drag.current.fade_in_frames = local;
                            } else {
                                drag.current.fade_out_frames = drag.frames - local;
                            }
                        }
                    }
                }
                let id = drag.id.clone();
                let envelope = drag.current.clone();
                self.dispatch(Command::UpdateProcessingPreview(processing::UpdateParams {
                    preview_id: id,
                    change: processing::Action::SetEnvelope { envelope },
                }));
                ui.ctx().request_repaint();
            }
            if response.drag_stopped()
                && self
                    .envelope_drag
                    .as_ref()
                    .is_some_and(|drag| drag.owner == target.owner && drag.handle == kind)
                && let Some(drag) = self.envelope_drag.take()
            {
                self.dispatch(Command::CommitProcessingPreview(
                    processing::PreviewParams {
                        preview_id: drag.id,
                    },
                ));
            }
            if response.has_focus() {
                self.envelope_focus = Some((target.owner.clone(), kind));
                let delta = ui.input_mut(|input| {
                    let modifiers = input.modifiers;
                    i32::from(
                        input.consume_key(modifiers, egui::Key::ArrowUp)
                            || input.consume_key(modifiers, egui::Key::ArrowRight),
                    ) - i32::from(
                        input.consume_key(modifiers, egui::Key::ArrowDown)
                            || input.consume_key(modifiers, egui::Key::ArrowLeft),
                    )
                });
                if delta != 0 {
                    let fine = ui.input(|input| input.modifiers.shift);
                    let mut next = envelope.clone();
                    match kind {
                        Handle::Gain => {
                            next.gain_db = Some(
                                (next.gain_db.unwrap_or(-60.0)
                                    + delta as f64 * if fine { 0.025 } else { 0.25 })
                                .clamp(-120.0, 60.0),
                            )
                        }
                        _ => {
                            let step = if fine { 1 } else { 10 };
                            let current = if kind == Handle::FadeIn {
                                next.fade_in_frames
                            } else {
                                next.fade_out_frames
                            };
                            let value = (current as i128 + delta as i128 * step)
                                .clamp(0, target.frames as i128)
                                as u64;
                            if kind == Handle::FadeIn {
                                next.fade_in_frames = value;
                            } else {
                                next.fade_out_frames = value;
                            }
                        }
                    }
                    self.envelope_edit(&target.owner, next);
                }
            }
        }
    }
    pub(super) fn envelope_modals(&mut self, ctx: &egui::Context) {
        if self
            .envelope_click
            .as_ref()
            .is_some_and(|click| ctx.input(|input| input.time) >= click.until)
            && let Some(click) = self.envelope_click.take()
        {
            self.open_envelope_entry(click.owner);
        }
        let Some(mut entry) = self.envelope_dialog.take() else {
            return;
        };
        let mut apply = false;
        let mut close = false;
        egui::Modal::new(egui::Id::unique("envelope-exact-entry")).show(ctx, |ui| {
            if self.host.frozen() {
                ui.visuals_mut().disabled_alpha = 1.0;
                ui.disable();
            }
            ui.heading("Gain and fades");
            ui.label(self.host.owner_name(&entry.owner));
            let mut mute = entry.draft.gain_db.is_none();
            if ui.checkbox(&mut mute, "Mute (−∞ dB)").changed() {
                entry.draft.gain_db = if mute { None } else { Some(0.0) };
            }
            ui.horizontal(|ui| {
                ui.label("Gain");
                if let Some(value) = &mut entry.draft.gain_db {
                    ui.add(
                        egui::DragValue::new(value)
                            .range(-120.0..=60.0)
                            .speed(0.1)
                            .suffix(" dB"),
                    );
                } else {
                    ui.label("−∞ dB");
                }
            });
            for (label, value) in [
                ("Fade in", &mut entry.draft.fade_in_frames),
                ("Fade out", &mut entry.draft.fade_out_frames),
            ] {
                ui.horizontal(|ui| {
                    ui.label(label);
                    ui.add(
                        egui::DragValue::new(value)
                            .range(0..=entry.frames)
                            .suffix(" frames"),
                    );
                    ui.label(format!("{:.3} s", *value as f64 / entry.rate as f64));
                });
            }
            if ui.button("Reset envelope").clicked() {
                entry.draft = Envelope::unity();
            }
            if let Some(error) = &entry.error {
                ui.colored_label(Color32::LIGHT_RED, error);
            }
            ui.horizontal(|ui| {
                apply = ui.button("Apply envelope").clicked();
                close = ui.button("Cancel").clicked();
            });
            close |= ui.input(|input| input.key_pressed(egui::Key::Escape));
        });
        if apply && !self.host.frozen() {
            let response = self.host.execute(Request::new(
                "envelope-exact",
                Command::EditProcessing(processing::EditParams {
                    owner: entry.owner.clone(),
                    expected_edit_revision: entry.revision.clone(),
                    change: processing::Action::SetEnvelope {
                        envelope: entry.draft.clone(),
                    },
                }),
            ));
            match response.outcome {
                Outcome::Ok { .. } => close = true,
                Outcome::Error { error } => entry.error = Some(error.message),
            }
        }
        if !close {
            self.envelope_dialog = Some(entry);
        }
    }
}
