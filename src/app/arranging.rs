use super::*;
use wavebob::arrangement::*;

#[derive(Clone)]
pub(super) enum Gesture {
    Insert {
        document: DocumentInfo,
        revision: u64,
    },
    Move {
        clip: Clip,
        offset: u64,
        revision: u64,
    },
    Trim {
        clip: Clip,
        left: bool,
        anchor: u64,
        revision: u64,
    },
    Select {
        anchor: u64,
        revision: u64,
    },
}
pub(super) struct TrackDraft {
    revision: String,
    name: String,
    gain: f64,
    pan: f64,
}
pub(super) struct ClipTrimDraft {
    pub revision: String,
    pub start: u64,
    pub end: u64,
}
fn frame_at(pos: Pos2, lane: Rect, view: &FrameRange) -> u64 {
    (view.start as f64
        + ((pos.x - lane.left()) / lane.width()) as f64 * (view.end - view.start) as f64)
        .round()
        .clamp(0.0, MAX_TIMELINE_FRAME as f64) as u64
}
fn clip_rect(clip: &Clip, lane: Rect, view: &FrameRange) -> Rect {
    let x = |frame: u64| {
        lane.left()
            + ((frame as f64 - view.start as f64) / (view.end - view.start) as f64) as f32
                * lane.width()
    };
    Rect::from_min_max(
        Pos2::new(x(clip.position), lane.top() + 8.0),
        Pos2::new(
            x(clip.position + clip.frames).max(x(clip.position) + 2.0),
            lane.bottom() - 8.0,
        ),
    )
}
impl WaveBob {
    fn timeline_view(&mut self, view: FrameRange) {
        self.dispatch(Command::SetTimelineViewport(TimelineViewportParams {
            start_frame: view.start,
            end_frame: view.end,
            expected_presentation_revision: None,
        }));
    }
    pub(super) fn navigate_arrangement(
        &mut self,
        cursor: u64,
        selection: Option<FrameRange>,
        selected_clip_ids: Vec<String>,
    ) {
        self.dispatch(Command::NavigateArrangement(NavigateParams {
            cursor,
            selection,
            selected_clip_ids,
        }));
    }
    pub(super) fn multitrack_view(&mut self, ui: &mut egui::Ui, help: &mut Help) {
        let snapshot = self.host.snapshot();
        let arrangement = snapshot.arrangement;
        let crossfades = arrangement.crossfades();
        let view = self.host.presentation().timeline;
        let frozen = self.host.frozen();
        let escape = ui
            .ctx()
            .input(|i| i.key_pressed(egui::Key::Escape) || !i.focused);
        if escape && !frozen {
            self.arrangement_gesture = None;
        }
        if let Some(gesture) = &self.arrangement_gesture {
            let revision = match gesture {
                Gesture::Insert { revision, .. }
                | Gesture::Move { revision, .. }
                | Gesture::Trim { revision, .. }
                | Gesture::Select { revision, .. } => *revision,
            };
            if revision != snapshot.revision && !frozen {
                self.arrangement_gesture = None;
            }
        }
        ui.horizontal(|ui| {
            ui.label(RichText::new(if arrangement.dirty {"Arrangement *"} else {"Arrangement"}).strong());
            let add=ui.button("+ Track");help.observe(&add,"Create an empty track. Drag open audio onto its timeline.");
            if add.clicked()&&!frozen {self.dispatch(Command::AddTrack(AddTrackParams::default()));}
            if ui.available_width()>300.0 {
                let draft=self.master_gain_draft.get_or_insert_with(||(arrangement.revision_id.clone(),arrangement.master_gain_db));
                if draft.0!=arrangement.revision_id {draft.0=arrangement.revision_id.clone();draft.1=arrangement.master_gain_db;}
                ui.label(RichText::new("Master").small().color(MUTED));
                let response=ui.add(egui::DragValue::new(&mut draft.1).range(-120.0..=24.0).speed(0.1).suffix(" dB"));
                let gain=draft.1;help.observe(&response,"Master output gain. Drag to adjust; release commits one undoable edit.");
                if !frozen&&(response.drag_stopped()||(response.changed()&&!response.dragged())) {self.dispatch(Command::SetMasterGain(wavebob::mixing::MasterGainParams{gain_db:gain}));}
            }
            ui.with_layout(egui::Layout::right_to_left(Align::Center),|ui| {
                for (label,factor) in [("Fit",0.0),("+",2.0),("−",0.5)] {
                    let response=ui.button(label);help.observe(&response,"Fit the arrangement, or zoom its timeline. Ctrl/Cmd-wheel zooms; wheel pans.");
                    if response.clicked()&&!frozen {
                        self.timeline_view(if factor==0.0 {FrameRange{start:0,end:(arrangement.frames()+arrangement.sample_rate as u64).max(arrangement.sample_rate as u64)}} else {zoom(&view,factor,0.5,MAX_TIMELINE_FRAME)});
                    }
                }
            });
        });
        ui.add_space(6.0);
        let width = ui.available_width();
        let controls = 154.0_f32.min(width * 0.35);
        let (ruler, rr) = ui.allocate_exact_size(Vec2::new(width, 22.0), Sense::click_and_drag());
        let time_ruler = Rect::from_min_max(ruler.min + Vec2::new(controls, 0.0), ruler.max);
        visuals::ruler(
            &ui.painter_at(time_ruler),
            time_ruler,
            view.start as f64 / arrangement.sample_rate as f64,
            view.end as f64 / arrangement.sample_rate as f64,
        );
        help.observe(&rr,"Click to place the timeline cursor; drag to select time. Ctrl/Cmd-wheel zooms; wheel pans.");
        if rr.drag_started()
            && !frozen
            && let Some(pos) = rr.interact_pointer_pos()
        {
            let pos = ui.ctx().input(|i| i.pointer.press_origin().unwrap_or(pos));
            self.arrangement_gesture = Some(Gesture::Select {
                anchor: frame_at(pos, time_ruler, &view),
                revision: snapshot.revision,
            });
        }
        if rr.clicked()
            && !frozen
            && let Some(pos) = rr.interact_pointer_pos()
        {
            self.navigate_arrangement(frame_at(pos, time_ruler, &view), None, Vec::new());
        }
        let scroll = ui.ctx().input(|i| i.smooth_scroll_delta);
        let hovered = ui.rect_contains_pointer(ui.max_rect());
        if hovered && !frozen && self.arrangement_gesture.is_none() {
            let (modifiers, pinch, pointer) = ui
                .ctx()
                .input(|i| (i.modifiers, i.zoom_delta(), i.pointer.hover_pos()));
            if pinch != 1.0 || ((modifiers.ctrl || modifiers.command) && scroll.y != 0.0) {
                let factor = if pinch != 1.0 {
                    pinch as f64
                } else {
                    (scroll.y as f64 * 0.008).exp()
                };
                let anchor = pointer.map_or(0.5, |p| {
                    ((p.x - time_ruler.left()) / time_ruler.width()).clamp(0.0, 1.0) as f64
                });
                self.timeline_view(zoom(&view, factor, anchor, MAX_TIMELINE_FRAME));
            } else if scroll.x != 0.0 || (modifiers.shift && scroll.y != 0.0) {
                self.timeline_view(pan(
                    &view,
                    -(scroll.x + scroll.y) as f64 * (view.end - view.start) as f64
                        / time_ruler.width() as f64,
                    MAX_TIMELINE_FRAME,
                ));
            }
        }
        let mut lanes = Vec::new();
        let mut remaining_waveforms = 4;
        egui::ScrollArea::vertical().id_salt("arrangement-tracks").auto_shrink([false,false]).show(ui,|ui| {
            let lane_height=((ui.available_height()-12.0)/4.0).clamp(96.0,150.0);
            if arrangement.tracks.is_empty() {ui.add_space(40.0);ui.label(RichText::new("Add a track, then drag an open audio file onto its timeline.").color(MUTED));}
            for (index,track) in arrangement.tracks.iter().enumerate() {
                let (row,_)=ui.allocate_exact_size(Vec2::new(width,lane_height),Sense::hover());
                if !row.intersects(ui.clip_rect()){continue;}
                let header=Rect::from_min_max(row.min,Pos2::new(row.left()+controls,row.bottom()));
                let lane=Rect::from_min_max(header.right_top(),row.max);lanes.push((track.id.clone(),lane));
                let painter=ui.painter_at(row);
                painter.rect_filled(header,0.0,Color32::from_rgb(23,30,37));
                visuals::grid(&painter,lane);painter.hline(row.x_range(),row.bottom(),Stroke::new(1.0,BORDER));
                painter.line_segment([header.right_top(),header.right_bottom()],Stroke::new(1.0,BORDER));
                let draft=self.track_drafts.entry(track.id.clone()).or_insert_with(||TrackDraft{revision:arrangement.revision_id.clone(),name:track.name.clone(),gain:track.gain_db,pan:track.pan});
                if draft.revision!=arrangement.revision_id {draft.revision=arrangement.revision_id.clone();draft.name=track.name.clone();draft.gain=track.gain_db;draft.pan=track.pan;}
                let mut command=None;
                let mut update=UpdateTrackParams{track_id:track.id.clone(),name:None,index:None,mute:None,solo:None,gain_db:None,pan:None};
                let mut controls_ui=child(ui,header.shrink2(Vec2::new(10.0,6.0)));
                controls_ui.horizontal(|ui| {
                    let label=ui.label(RichText::new(format!("Track {}",index+1)).small().color(MUTED));
                    label.context_menu(|ui| {
                        if ui.add_enabled(index>0,egui::Button::new("Move up")).clicked(){update.index=Some(index-1);ui.close();}
                        if ui.add_enabled(index+1<arrangement.tracks.len(),egui::Button::new("Move down")).clicked(){update.index=Some(index+1);ui.close();}
                        if ui.button("Remove track").clicked(){command=Some(Command::RemoveTrack(TrackParams{track_id:track.id.clone()}));ui.close();}
                    });
                    help.observe(&label,"Right-click for track ordering and removal. Removing a track and all clips is one undoable edit.");
                    ui.with_layout(egui::Layout::right_to_left(Align::Center),|ui| {
                        for (label,state) in [("S",track.solo),("M",track.mute)] {
                            let response=ui.add(egui::Button::new(label).selected(state).min_size(Vec2::new(21.0,21.0)));
                            help.observe(&response,"Solo includes only soloed tracks; mute always takes precedence over solo.");
                            if response.clicked(){if label=="S"{update.solo=Some(!state);}else{update.mute=Some(!state);}}
                        }
                    });
                });
                let name=controls_ui.add(egui::TextEdit::singleline(&mut draft.name).desired_width(controls-20.0).frame(egui::Frame::NONE));
                help.observe(&name,"Rename this track; commit by leaving the text field.");
                if name.lost_focus()&&draft.name!=track.name {update.name=Some(draft.name.clone());}
                controls_ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width=(controls-95.0).max(25.0);
                    let gain=ui.add(egui::Slider::new(&mut draft.gain,-60.0..=12.0).show_value(false));help.observe(&gain,"Track gain. Release the slider to commit one undoable edit.");
                    if gain.drag_stopped() || (gain.changed()&&!gain.dragged()) {update.gain_db=Some(draft.gain);}
                    ui.label(RichText::new(format!("{:.1} dB",draft.gain)).size(10.0).color(MUTED));
                    let (knob,response)=ui.allocate_exact_size(Vec2::splat(25.0),Sense::drag());
                    if response.dragged(){draft.pan=(draft.pan+response.drag_delta().x as f64/50.0).clamp(-1.0,1.0);}
                    if response.drag_stopped(){update.pan=Some(draft.pan);}
                    let center=knob.center();ui.painter().circle(center,10.0,SURFACE,Stroke::new(1.0,BORDER));
                    let angle=draft.pan as f32*2.3;ui.painter().line_segment([center,center+Vec2::new(angle.sin(),-angle.cos())*8.0],Stroke::new(1.5,BLUE));
                    help.observe(&response,"Track pan: drag left/right. Center preserves stereo balance.");
                });
                if update.name.is_some()||update.index.is_some()||update.mute.is_some()||update.solo.is_some()||update.gain_db.is_some()||update.pan.is_some(){command=Some(Command::UpdateTrack(update));}
                if let Some(command)=command && !frozen {self.dispatch(command);}
                let blank=ui.interact(lane,ui.scope_id().with((&track.id,"lane")),Sense::click_and_drag());
                help.observe(&blank,"Drag audio here. Two overlapping clips crossfade automatically; a third simultaneous clip is rejected. Drag empty time to select.");
                if blank.drag_started()&&!frozen&&let Some(pos)=ui.ctx().input(|i|i.pointer.press_origin()) {self.arrangement_gesture=Some(Gesture::Select{anchor:frame_at(pos,lane,&view),revision:snapshot.revision});}
                if blank.clicked()&&!frozen&&let Some(pos)=blank.interact_pointer_pos(){self.navigate_arrangement(frame_at(pos,lane,&view),None,Vec::new());}
                let color=[visuals::TEAL,visuals::WAVE,visuals::GOLD][index%3];
                if track.clips.is_empty() {
                    let empty=lane.shrink2(Vec2::new(8.0,8.0));
                    for (start,end) in [(empty.left_top(),empty.right_top()),(empty.right_top(),empty.right_bottom()),(empty.right_bottom(),empty.left_bottom()),(empty.left_bottom(),empty.left_top())] {
                        painter.extend(egui::Shape::dashed_line(&[start,end],Stroke::new(1.0,BORDER),5.0,4.0));
                    }
                    painter.text(empty.center(),egui::Align2::CENTER_CENTER,"Drag audio here",egui::FontId::proportional(11.0),MUTED);
                }
                for clip in &track.clips {
                    let rect=clip_rect(clip,lane,&view);let visible=rect.intersect(lane).intersect(ui.clip_rect());
                    if !visible.is_positive(){continue;}
                    let selected=arrangement.selected_clip_ids.contains(&clip.id);
                    let painter=ui.painter_at(lane);
                    painter.rect_filled(rect,3.0,color.gamma_multiply(if selected{0.24}else{0.14}));
                    painter.rect_stroke(rect,3.0,Stroke::new(if selected{2.0}else{1.0},color),egui::StrokeKind::Inside);
                    if self.clip_waves.get(&clip.id).is_none_or(|wave|wave.revision_id!=clip.edit_revision_id) {
                        if remaining_waveforms>0 {
                        remaining_waveforms-=1;
                        let result=self.host.execute(Request::new("clip-peaks",Command::GetClipWaveform(ClipWaveformParams{clip_id:clip.id.clone(),bins:512})));
                        if let Outcome::Ok{result:CommandResult::Waveform(wave)}=result.outcome {self.clip_waves.insert(clip.id.clone(),wave);}
                        } else {ui.ctx().request_repaint();}
                    }
                    if let Some(wave)=self.clip_waves.get(&clip.id)&&let Some(peaks)=wave.channels.first(){visuals::peaks(&painter,Rect::from_min_max(rect.min+Vec2::new(2.0,23.0),rect.max-Vec2::splat(3.0)),peaks,color);}
                    let next_start=track.clips.iter().filter(|other|other.position>clip.position).map(|other|lane.left()+(other.position as f64-view.start as f64) as f32/(view.end-view.start) as f32*lane.width()).fold(rect.right(),f32::min);
                    let title_right=rect.right().min(next_start).min(lane.right());
                    let title_width=(title_right-rect.left()-12.0).max(0.0);
                    let mut title=egui::text::LayoutJob::simple_singleline(clip.name.clone(),egui::FontId::proportional(11.0),Color32::WHITE);
                    title.wrap=egui::text::TextWrapping::truncate_at_width(title_width);
                    let title=ui.fonts_mut(|fonts|fonts.layout_job(title));
                    painter.with_clip_rect(rect.intersect(lane)).galley(rect.min+Vec2::new(8.0,6.0),title,Color32::WHITE);
                    let response=ui.interact(visible,ui.scope_id().with(&clip.id),Sense::click_and_drag());
                    response.context_menu(|ui| {
                        if ui.button("Remove clip").clicked()&&!frozen {self.dispatch(Command::RemoveClip(ClipParams{clip_id:clip.id.clone()}));ui.close();}
                    });
                    help.observe(&response,"Drag this pinned clip in time or onto another track. Invalid/off-track drops cancel. Ctrl/Cmd-click selects multiple clips.");
                    if response.clicked()&&!frozen {
                        let mut selected_ids=if ui.ctx().input(|i|i.modifiers.ctrl||i.modifiers.command){arrangement.selected_clip_ids.clone()}else{Vec::new()};
                        if selected_ids.contains(&clip.id){selected_ids.retain(|id|id!=&clip.id);}else{selected_ids.push(clip.id.clone());}
                        self.navigate_arrangement(clip.position,None,selected_ids);
                    }
                    if response.drag_started()&&!frozen&&let Some(pos)=ui.ctx().input(|i|i.pointer.press_origin()) {self.arrangement_gesture=Some(Gesture::Move{clip:clip.clone(),offset:frame_at(pos,lane,&view).saturating_sub(clip.position),revision:snapshot.revision});}
                    if rect.width()>16.0 {
                        for left in [true,false] {
                            let edge=if left {Rect::from_min_max(rect.min,Pos2::new(rect.left()+6.0,rect.bottom()))}else{Rect::from_min_max(Pos2::new(rect.right()-6.0,rect.top()),rect.max)}.intersect(visible);
                            let handle=ui.interact(edge,ui.scope_id().with((&clip.id,left)),Sense::drag());
                            if handle.hovered()||handle.dragged(){ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);}
                            help.observe(&handle,"Drag this edge to trim the pinned source interval. Release commits one edit; Escape cancels.");
                            if handle.drag_started()&&!frozen&&let Some(pos)=ui.ctx().input(|i|i.pointer.press_origin()) {self.arrangement_gesture=Some(Gesture::Trim{clip:clip.clone(),left,anchor:frame_at(pos,lane,&view),revision:snapshot.revision});}
                        }
                    }
                }
                for region in crossfades.regions.iter().filter(|region|region.track_id==track.id) {
                    let x=|frame:u64|lane.left()+(frame as f64-view.start as f64) as f32/(view.end-view.start) as f32*lane.width();
                    let overlap=Rect::from_min_max(Pos2::new(x(region.start),lane.top()+27.0),Pos2::new(x(region.end),lane.bottom()-9.0));
                    let painter=ui.painter_at(lane);
                    painter.rect_filled(overlap,0.0,color.gamma_multiply(0.10));
                    for voice in 0..2 {
                        let points=(0..=32).map(|i|{let frame=region.start+(region.end-region.start-1)*i/32;let gain=region.gains(frame)[voice];Pos2::new(overlap.left()+overlap.width()*i as f32/32.0,overlap.bottom()-overlap.height()*gain as f32)}).collect::<Vec<_>>();
                        painter.add(egui::Shape::line(points,Stroke::new(1.2,color)));
                    }
                    if overlap.width()>95.0 {painter.rect_filled(Rect::from_center_size(Pos2::new(overlap.center().x,lane.top()+7.0),Vec2::new(if region.kind==CrossfadeKind::Contained{115.0}else{90.0},13.0)),2.0,SURFACE);painter.text(Pos2::new(overlap.center().x,lane.top()+1.0),egui::Align2::CENTER_TOP,if region.kind==CrossfadeKind::Contained{"Auto crossfade · dip"}else{"Auto crossfade"},egui::FontId::proportional(10.0),Color32::WHITE);}
                    let response=ui.interact(overlap.intersect(lane),ui.scope_id().with((&region.first_clip_id,&region.second_clip_id,"crossfade-help")),Sense::hover());help.observe(&response,"Automatic equal-power transition derived from the clip overlap. Moving or trimming clips updates both the sound and these curves.");
                }
                if let Some(selection)=&arrangement.selection {
                    let x=|frame:u64|lane.left()+(frame as f64-view.start as f64) as f32/(view.end-view.start) as f32*lane.width();
                    painter.rect_filled(Rect::from_min_max(Pos2::new(x(selection.start),lane.top()),Pos2::new(x(selection.end),lane.bottom())).intersect(lane),0.0,BLUE.gamma_multiply(0.1));
                }
                let x=lane.left()+(arrangement.cursor as f64-view.start as f64) as f32/(view.end-view.start) as f32*lane.width();
                if lane.x_range().contains(x){painter.vline(x,lane.y_range(),Stroke::new(1.0,Color32::WHITE));}
            }
        });
        self.clip_waves
            .retain(|id, _| arrangement.clip(id).is_some());
        self.host
            .set_transient_resources(if self.arrangement_gesture.is_some() {
                vec!["arrangement-gesture-preview".into()]
            } else {
                Vec::new()
            });
        if frozen {
            return;
        }
        if ui
            .ctx()
            .input(|i| !i.pointer.primary_down() && !i.pointer.any_released())
        {
            self.arrangement_gesture = None;
        }
        if let Some(gesture) = self.arrangement_gesture.clone()
            && let Some(pos) = ui.ctx().input(|i| i.pointer.interact_pos())
        {
            let released = ui.ctx().input(|i| i.pointer.any_released());
            let lane = lanes.iter().find(|(_, rect)| rect.contains(pos));
            let command = match gesture {
                Gesture::Insert { document, revision } => lane.map(|(id, rect)| {
                    let position = frame_at(pos, *rect, &view);
                    let frames = ((document.frames as u128 * 48000
                        + document.sample_rate as u128 / 2)
                        / document.sample_rate as u128) as u64;
                    let ghost = Clip {
                        id: String::new(),
                        name: document.name.clone(),
                        document_id: document.id.clone(),
                        revision_id: document.revision_id.clone(),
                        edit_revision_id: String::new(),
                        insertion_order: 0,
                        processed_revision_id: None,
                        effects: Vec::new(),
                        source_frames: document.frames,
                        source_start: 0,
                        source_end: document.frames,
                        source_rate: document.sample_rate,
                        channels: document.channels,
                        position,
                        frames,
                    };
                    paint_preview(ui, *rect, &view, &ghost);
                    (
                        Command::InsertClip(InsertClipParams {
                            track_id: id.clone(),
                            document_id: document.id,
                            revision_id: document.revision_id,
                            source_start: 0,
                            source_end: document.frames,
                            position,
                        }),
                        revision,
                    )
                }),
                Gesture::Move {
                    mut clip,
                    offset,
                    revision,
                } => lane.map(|(id, rect)| {
                    clip.position = frame_at(pos, *rect, &view).saturating_sub(offset);
                    paint_preview(ui, *rect, &view, &clip);
                    (
                        Command::MoveClip(MoveClipParams {
                            clip_id: clip.id,
                            track_id: id.clone(),
                            position: clip.position,
                        }),
                        revision,
                    )
                }),
                Gesture::Select { anchor, revision } => {
                    let end = frame_at(pos, time_ruler, &view);
                    let start = anchor.min(end);
                    let end = anchor.max(end);
                    let x = |f: u64| {
                        time_ruler.left()
                            + (f as f64 - view.start as f64) as f32 / (view.end - view.start) as f32
                                * time_ruler.width()
                    };
                    for (_, lane) in &lanes {
                        ui.painter_at(*lane).rect_filled(
                            Rect::from_min_max(
                                Pos2::new(x(start), lane.top()),
                                Pos2::new(x(end), lane.bottom()),
                            ),
                            0.0,
                            BLUE.gamma_multiply(0.15),
                        );
                    }
                    Some((
                        Command::NavigateArrangement(NavigateParams {
                            cursor: start,
                            selection: (start < end).then_some(FrameRange { start, end }),
                            selected_clip_ids: Vec::new(),
                        }),
                        revision,
                    ))
                }
                Gesture::Trim {
                    mut clip,
                    left,
                    anchor,
                    revision,
                } => lane
                    .filter(|(id, _)| {
                        arrangement
                            .clip(&clip.id)
                            .is_some_and(|(track, _)| &track.id == id)
                    })
                    .and_then(|(_, rect)| {
                        let delta = frame_at(pos, *rect, &view) as i128 - anchor as i128;
                        let shift = (delta.abs() * clip.source_rate as i128
                            + arrangement.sample_rate as i128 / 2)
                            / arrangement.sample_rate as i128
                            * delta.signum();
                        let mut start = clip.source_start;
                        let mut end = clip.source_end;
                        if left {
                            let min = clip.source_start.saturating_sub(
                                (clip.position as u128 * clip.source_rate as u128
                                    / arrangement.sample_rate as u128)
                                    as u64,
                            );
                            start = (clip.source_start as i128 + shift)
                                .clamp(min as i128, clip.source_end as i128 - 1)
                                as u64;
                        } else {
                            end = (clip.source_end as i128 + shift)
                                .clamp(clip.source_start as i128 + 1, clip.source_frames as i128)
                                as u64;
                        }
                        clip = clip.trimmed(start, end, arrangement.sample_rate).ok()?;
                        paint_preview(ui, *rect, &view, &clip);
                        Some((
                            Command::TrimClip(TrimClipParams {
                                clip_id: clip.id,
                                edit_revision_id: clip.edit_revision_id,
                                source_start: clip.source_start,
                                source_end: clip.source_end,
                            }),
                            revision,
                        ))
                    }),
            };
            if released {
                self.arrangement_gesture = None;
                if let Some((command, revision)) = command {
                    let response = self.host.execute(
                        Request::new("timeline-drop", command).expecting_revision(revision),
                    );
                    if let Outcome::Error { error } = response.outcome {
                        self.error = Some(error.message);
                    }
                }
            }
        }
    }
}
fn paint_preview(ui: &egui::Ui, lane: Rect, view: &FrameRange, clip: &Clip) {
    let rect = clip_rect(clip, lane, view);
    let painter = ui.painter_at(lane);
    painter.rect_filled(rect, 3.0, BLUE.gamma_multiply(0.25));
    painter.rect_stroke(rect, 3.0, Stroke::new(2.0, BLUE), egui::StrokeKind::Inside);
    painter.text(
        rect.min + Vec2::splat(8.0),
        egui::Align2::LEFT_TOP,
        &clip.name,
        egui::FontId::proportional(11.0),
        Color32::WHITE,
    );
}
