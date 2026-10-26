use super::*;
impl Session {
    #[cfg(feature = "gui")]
    pub(crate) fn prepare_arrangement_view(&mut self, resolve_preview: bool) {
        if resolve_preview {
            self.editor.resolve_preview_for_arrangement();
        }
        let key = self.editor.required_mix_identity();
        match self.editor.ensure_mix(resolve_preview) {
            Ok(_) => {
                if resolve_preview
                    || self
                        .mix_failure
                        .as_ref()
                        .is_some_and(|(old, _)| old != &key)
                {
                    self.mix_failure = None;
                }
            }
            Err(error) if error.code != ErrorCode::Busy => self.mix_failure = Some((key, error)),
            Err(_) => {}
        }
    }

    pub fn mix_snapshot(&self) -> mixing::MixState {
        let transport = self.playback.snapshot();
        let active =
            transport.requested.document_id.as_ref() == Some(&self.editor.arrangement().id);
        let mut snapshot = self.editor.mix_state(
            self.pending_mix_play || (active && transport.requested.playing),
            if active { transport.peak_level } else { 0.0 },
        );
        if let Some((revision, error)) = &self.mix_failure
            && revision == &snapshot.required_render_identity
        {
            snapshot.preparation_error = Some(error.clone());
        }
        snapshot
    }
    pub(super) fn play_mix(&mut self, toggle: bool) -> Result<CommandResult, CommandError> {
        self.playback.ready()?;
        let arrangement = self.editor.arrangement();
        let active =
            self.playback.snapshot().requested.document_id.as_ref() == Some(&arrangement.id);
        if toggle
            && (self.pending_mix_play || (active && self.playback.snapshot().requested.playing))
        {
            self.pause_output()?;
            return Ok(CommandResult::Mix(self.mix_snapshot().into()));
        }
        if active && self.playback.snapshot().requested.playing {
            return Ok(CommandResult::Mix(self.mix_snapshot().into()));
        }
        if arrangement.frames() == 0 {
            return Err(CommandError::new(
                ErrorCode::InvalidParams,
                "An empty arrangement cannot play",
            ));
        }
        let revision = self.next_revision()?;
        self.editor.ensure_mix(true)?;
        self.mix_failure = None;
        self.browser.stop();
        self.playback.pause()?;
        self.pending_mix_play = true;
        self.pending_mix_seek = None;
        self.last_active_document = self.editor.active();
        if arrangement.cursor >= self.editor.mix_document(self.snapshot.loop_enabled).frames {
            let _ = self.editor.execute(
                Command::NavigateArrangement(arrangement::NavigateParams {
                    cursor: 0,
                    selection: arrangement.selection,
                    selected_clip_ids: arrangement.selected_clip_ids,
                }),
                &mut self.snapshot.revision,
            )?;
        }
        self.snapshot.revision = revision;
        self.synchronize_mix(None);
        Ok(CommandResult::Mix(self.mix_snapshot().into()))
    }
    pub(super) fn seek_mix(&mut self, frame: u64) -> Result<CommandResult, CommandError> {
        let arrangement = self.editor.arrangement();
        let before = self.snapshot.revision;
        let result = self.editor.execute(
            Command::NavigateArrangement(arrangement::NavigateParams {
                cursor: frame,
                selection: arrangement.selection,
                selected_clip_ids: arrangement.selected_clip_ids,
            }),
            &mut self.snapshot.revision,
        )?;
        let seek = self.output_seek_needed(&arrangement.id, frame);
        if seek && self.snapshot.revision == before {
            self.snapshot.revision = self.next_revision()?;
        }
        self.synchronize_mix(Some(&(arrangement.id, frame)));
        Ok(result)
    }
    /// Returns true when arrangement transport owns synchronization this frame.
    pub(super) fn synchronize_mix(&mut self, seek: Option<&(String, u64)>) -> bool {
        let arrangement = self.editor.arrangement();
        let transport = self.playback.snapshot();
        if !self.pending_mix_play
            && transport.requested.document_id.as_ref() != Some(&arrangement.id)
        {
            return false;
        }
        if self
            .mix_failure
            .as_ref()
            .is_some_and(|(revision, _)| revision == &self.editor.required_mix_identity())
        {
            return true;
        }
        if arrangement.frames() == 0 {
            self.pending_mix_play = false;
            let _ = self.playback.clear();
            return true;
        }
        let explicit = seek
            .filter(|(id, _)| id == &arrangement.id)
            .map(|(_, frame)| *frame)
            .or(self.pending_mix_seek);
        if explicit.is_some() {
            self.pending_mix_seek = explicit;
        }
        let Some(graph) = self.editor.prepared_mix() else {
            if !self.pending_mix_play && !transport.requested.playing && explicit.is_none() {
                return true;
            }
            if let Err(error) = self.editor.ensure_mix(false)
                && error.code != ErrorCode::Busy
            {
                self.mix_failure = Some((self.editor.required_mix_identity(), error));
                self.pending_mix_play = false;
                let _ = self.playback.pause();
            }
            return true;
        };
        let document = self.editor.mix_document(self.snapshot.loop_enabled);
        if self.pending_mix_play
            || explicit.is_some()
            || self
                .playback
                .needs_refresh(&document, self.snapshot.loop_enabled, None)
        {
            let position = if self.pending_mix_play {
                Some(document.cursor)
            } else {
                explicit
            };
            match self.playback.request_mix(
                &document,
                graph,
                self.snapshot.loop_enabled,
                self.pending_mix_play || transport.requested.playing,
                position,
            ) {
                Ok(()) => {
                    self.pending_mix_play = false;
                    self.pending_mix_seek = None;
                }
                Err(error) => {
                    self.mix_failure = Some((self.editor.required_mix_identity(), error));
                    self.pending_mix_play = false;
                }
            }
        }
        true
    }
}
