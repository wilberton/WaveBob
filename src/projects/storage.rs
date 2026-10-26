//! Filesystem transactions and validation. Only background workers call this module.
use super::*;
use crate::{
    CommandError, ErrorCode,
    audio::{self, Audio},
    exports,
};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

pub(crate) struct SaveSnapshot {
    pub payload: Payload,
    pub revisions: BTreeMap<String, Arc<Audio>>,
    pub revision_id: String,
    pub protected_roots: Vec<String>,
}
pub(crate) struct LoadedProject {
    pub payload: Payload,
    pub revisions: BTreeMap<String, Arc<Audio>>,
    pub summary: ProjectSummary,
    pub asset_roots: Vec<String>,
}
struct StagedAsset {
    temporary: tempfile::NamedTempFile,
    destination: PathBuf,
    metadata: Asset,
}
pub(crate) struct PreparedProject {
    manifest: Option<tempfile::NamedTempFile>,
    destination: PathBuf,
    staged: Vec<StagedAsset>,
    asset_root: PathBuf,
    root_created: bool,
    pub summary: ProjectSummary,
}
impl Drop for PreparedProject {
    fn drop(&mut self) {
        self.staged.clear();
        self.manifest.take();
        if self.root_created {
            let _ = fs::remove_dir(&self.asset_root);
        }
    }
}
impl PreparedProject {
    pub fn publish(mut self, overwrite: bool) -> Result<ProjectSummary, CommandError> {
        for staged in self.staged.drain(..) {
            match staged.temporary.persist_noclobber(&staged.destination) {
                Ok(_) => {}
                Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                    // Another save may have published this identical immutable revision.
                    read_asset(
                        &staged.destination,
                        &staged.metadata,
                        &AtomicBool::new(false),
                        false,
                    )?;
                }
                Err(error) => return Err(exports::io_error(error.error)),
            }
        }
        let temporary = self.manifest.take().unwrap();
        if overwrite {
            temporary.persist(&self.destination)
        } else {
            temporary.persist_noclobber(&self.destination)
        }
        .map_err(|error| {
            CommandError::new(
                if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                    ErrorCode::OutputExists
                } else {
                    ErrorCode::IoError
                },
                format!("Publish project: {}", error.error),
            )
        })?;
        self.root_created = false;
        Ok(self.summary.clone())
    }
}
pub(crate) fn prepare_save(
    mut snapshot: SaveSnapshot,
    path: &str,
    overwrite: bool,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<PreparedProject, CommandError> {
    snapshot.payload.arrangement.saved_revision_id =
        Some(snapshot.payload.arrangement.revision_id.clone());
    snapshot.payload.arrangement.dirty = false;
    let path = Path::new(path);
    if !path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wavebob"))
    {
        return Err(invalid("Choose a project filename ending in .wavebob"));
    }
    let name = path
        .file_name()
        .ok_or_else(|| invalid("Choose a project filename"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .canonicalize()
        .map_err(exports::io_error)?;
    if snapshot
        .protected_roots
        .iter()
        .any(|root| parent.starts_with(root))
    {
        return Err(invalid(
            "Managed project assets are immutable; choose a project destination outside the asset directory",
        ));
    }
    let destination = parent.join(name);
    if !overwrite && destination.try_exists().map_err(exports::io_error)? {
        return Err(CommandError::new(
            ErrorCode::OutputExists,
            "Project exists; explicit overwrite intent is required",
        ));
    }
    let stem = path
        .file_stem()
        .unwrap()
        .to_str()
        .ok_or_else(|| invalid("Project path is not valid Unicode"))?;
    let root_name = format!("{stem}.assets");
    let root = parent.join(&root_name);
    let root_created = !root.try_exists().map_err(exports::io_error)?;
    fs::create_dir_all(&root).map_err(exports::io_error)?;
    let asset_root = root.canonicalize().map_err(exports::io_error)?;
    if asset_root.parent() != Some(parent.as_path()) {
        return Err(invalid(
            "Managed assets must stay inside the project directory",
        ));
    }
    let stored_root_name = asset_root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("Managed asset path is not valid Unicode"))?
        .to_string();
    let manifest = tempfile::NamedTempFile::new_in(&parent).map_err(exports::io_error)?;
    let mut prepared = PreparedProject {
        manifest: Some(manifest),
        destination: destination.clone(),
        staged: Vec::new(),
        asset_root: asset_root.clone(),
        root_created,
        summary: ProjectSummary {
            path: unicode(&destination)?,
            revision_id: snapshot.revision_id,
            schema_version: SCHEMA_VERSION,
            documents: snapshot.payload.documents.len(),
            tracks: snapshot.payload.arrangement.tracks.len(),
            clips: snapshot
                .payload
                .arrangement
                .tracks
                .iter()
                .map(|t| t.clips.len())
                .sum(),
            assets: snapshot.revisions.len(),
            assets_written: 0,
            assets_reused: 0,
            managed_assets_path: unicode(&asset_root)?,
        },
    };
    let count = snapshot.revisions.len().max(1);
    for (index, (revision, audio)) in snapshot.revisions.into_iter().enumerate() {
        exports::check_cancel(cancel)?;
        canonical_id(&revision)?;
        let checksum = audio_checksum(&audio, cancel)?;
        let file_name = format!("{revision}.wav");
        let destination = asset_root.join(&file_name);
        let mut metadata = Asset {
            revision_id: revision,
            file: format!("{stored_root_name}/{file_name}"),
            checksum,
            sample_rate: audio.rate,
            channels: audio.channels,
            frames: audio.frames(),
            encoding: audio.encoding.clone(),
            source_details: audio.source.clone(),
        };
        if destination.try_exists().map_err(exports::io_error)? {
            let canonical = destination.canonicalize().map_err(exports::io_error)?;
            if !canonical.starts_with(&asset_root) {
                return Err(invalid("Managed asset points outside its asset directory"));
            }
            metadata.file = format!(
                "{stored_root_name}/{}",
                canonical
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| invalid("Managed asset filename is not valid Unicode"))?
            );
            read_asset(&canonical, &metadata, cancel, false)?;
            prepared.summary.assets_reused += 1;
        } else {
            let mut temporary =
                tempfile::NamedTempFile::new_in(&asset_root).map_err(exports::io_error)?;
            exports::write_wav(
                temporary.as_file_mut(),
                &audio.samples,
                audio.rate,
                audio.channels,
                crate::exports::WavEncoding::Float32,
                cancel,
                |value| progress(((index * 85 + value as usize * 85 / 100) / count) as u8),
            )?;
            temporary.as_file_mut().flush().map_err(exports::io_error)?;
            temporary.as_file().sync_all().map_err(exports::io_error)?;
            prepared.staged.push(StagedAsset {
                temporary,
                destination,
                metadata: metadata.clone(),
            });
            prepared.summary.assets_written += 1;
        }
        snapshot.payload.assets.push(metadata);
        progress(((index + 1) * 85 / count) as u8);
    }
    exports::check_cancel(cancel)?;
    let file = ProjectFile {
        schema_version: SCHEMA_VERSION,
        checksum: checksum(&snapshot.payload),
        payload: snapshot.payload,
    };
    let temporary = prepared.manifest.as_mut().unwrap();
    serde_json::to_writer_pretty(temporary.as_file_mut(), &file)
        .map_err(|error| invalid(&format!("Serialize project: {error}")))?;
    temporary
        .as_file_mut()
        .write_all(b"\n")
        .map_err(exports::io_error)?;
    temporary.as_file_mut().flush().map_err(exports::io_error)?;
    temporary.as_file().sync_all().map_err(exports::io_error)?;
    if temporary
        .as_file()
        .metadata()
        .map_err(exports::io_error)?
        .len()
        > 16 * 1024 * 1024
    {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Project manifest exceeds 16 MiB",
        ));
    }
    progress(95);
    Ok(prepared)
}
fn digest_header(rate: u32, channels: u16, frames: u64) -> Sha256 {
    let mut digest = Sha256::new();
    digest.update(rate.to_le_bytes());
    digest.update(channels.to_le_bytes());
    digest.update(frames.to_le_bytes());
    digest
}
fn audio_checksum(audio: &Audio, cancel: &AtomicBool) -> Result<String, CommandError> {
    let mut digest = digest_header(audio.rate, audio.channels, audio.frames());
    let mut bytes = [0u8; 16384];
    for chunk in audio.samples.chunks(4096) {
        exports::check_cancel(cancel)?;
        for (index, value) in chunk.iter().enumerate() {
            bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        digest.update(&bytes[..chunk.len() * 4]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn read_asset(
    path: &Path,
    metadata: &Asset,
    cancel: &AtomicBool,
    collect: bool,
) -> Result<Option<Audio>, CommandError> {
    if metadata.sample_rate == 0
        || !(1..=2).contains(&metadata.channels)
        || metadata.frames.saturating_mul(metadata.channels as u64) > 128 * 1024 * 1024
    {
        return Err(invalid("Invalid managed audio dimensions"));
    }
    let mut reader = hound::WavReader::open(path).map_err(|error| {
        CommandError::new(
            if matches!(&error,hound::Error::IoError(io) if io.kind()==std::io::ErrorKind::NotFound)
            {
                ErrorCode::MissingAsset
            } else {
                ErrorCode::InvalidProject
            },
            format!("Read managed asset {}: {error}", path.display()),
        )
    })?;
    let spec = reader.spec();
    let values = metadata.frames * metadata.channels as u64;
    if spec.sample_rate != metadata.sample_rate
        || spec.channels != metadata.channels
        || spec.sample_format != hound::SampleFormat::Float
        || spec.bits_per_sample != 32
        || reader.len() as u64 != values
    {
        return Err(invalid(
            "Managed asset encoding/duration does not match its manifest",
        ));
    }
    let mut samples = Vec::new();
    if collect {
        samples.try_reserve_exact(values as usize).map_err(|_| {
            CommandError::new(ErrorCode::ResourceLimit, "Unable to allocate managed audio")
        })?;
    }
    let mut digest = digest_header(metadata.sample_rate, metadata.channels, metadata.frames);
    let mut bytes = [0u8; 16384];
    let mut buffered = 0;
    for (index, value) in reader.samples::<f32>().enumerate() {
        if index.is_multiple_of(4096) {
            exports::check_cancel(cancel)?;
        }
        let value = value.map_err(|error| invalid(&format!("Decode managed asset: {error}")))?;
        if !value.is_finite() {
            return Err(invalid("Managed audio contains nonfinite values"));
        }
        if collect {
            samples.push(value);
        }
        bytes[buffered..buffered + 4].copy_from_slice(&value.to_le_bytes());
        buffered += 4;
        if buffered == bytes.len() {
            digest.update(bytes);
            buffered = 0;
        }
    }
    digest.update(&bytes[..buffered]);
    if format!("{:x}", digest.finalize()) != metadata.checksum {
        return Err(invalid(&format!(
            "Managed asset checksum mismatch: {}",
            path.display()
        )));
    }
    if collect {
        audio::prepare(
            metadata.sample_rate,
            metadata.channels,
            metadata.encoding.clone(),
            metadata.source_details.clone(),
            samples,
            cancel,
            |_| {},
        )
        .map(Some)
    } else {
        Ok(None)
    }
}
pub(crate) fn load(
    path: &str,
    available_bytes: u64,
    cancel: &AtomicBool,
    progress: impl Fn(u8),
) -> Result<LoadedProject, CommandError> {
    let path = Path::new(path).canonicalize().map_err(exports::io_error)?;
    let parent = path.parent().unwrap();
    let file = fs::File::open(&path).map_err(exports::io_error)?;
    if file.metadata().map_err(exports::io_error)?.len() > 16 * 1024 * 1024 {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Project manifest exceeds 16 MiB",
        ));
    }
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(exports::io_error)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(invalid("Project manifest grew beyond its limit"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|error| invalid(&format!("Read project JSON: {error}")))?;
    let version = value["schema_version"]
        .as_u64()
        .ok_or_else(|| invalid("Project schema_version is missing"))?;
    if version != SCHEMA_VERSION as u64 {
        return Err(CommandError::new(
            ErrorCode::UnsupportedProjectVersion,
            format!("Project schema {version} is unsupported; expected {SCHEMA_VERSION}"),
        ));
    }
    let mut file: ProjectFile = serde_json::from_value(value)
        .map_err(|error| invalid(&format!("Invalid project manifest: {error}")))?;
    if checksum(&file.payload) != file.checksum {
        return Err(invalid("Project manifest checksum mismatch"));
    }
    let total = file
        .payload
        .assets
        .iter()
        .try_fold(0u64, |total, asset| {
            total.checked_add(
                asset
                    .frames
                    .checked_mul(asset.channels as u64)?
                    .checked_mul(4)?,
            )
        })
        .ok_or_else(|| invalid("Managed audio dimensions overflow"))?;
    if total > available_bytes {
        return Err(CommandError::new(
            ErrorCode::ResourceLimit,
            "Loading this project would exceed the retained audio budget; close audio before retrying",
        ));
    }
    let mut revisions = BTreeMap::new();
    let count = file.payload.assets.len().max(1);
    let mut roots = BTreeSet::new();
    for (index, asset) in file.payload.assets.iter().enumerate() {
        exports::check_cancel(cancel)?;
        canonical_id(&asset.revision_id)?;
        let relative = Path::new(&asset.file);
        if relative
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
            || relative.components().count() < 2
        {
            return Err(invalid(
                "Managed asset paths must be relative paths inside the project directory",
            ));
        }
        let joined = parent.join(relative);
        let canonical = joined.canonicalize().map_err(|error| {
            CommandError::new(
                if error.kind() == std::io::ErrorKind::NotFound {
                    ErrorCode::MissingAsset
                } else {
                    ErrorCode::IoError
                },
                format!("Read managed asset {}: {error}", joined.display()),
            )
        })?;
        let root = parent
            .join(relative.components().next().unwrap().as_os_str())
            .canonicalize()
            .map_err(exports::io_error)?;
        if root == parent || !root.starts_with(parent) || !canonical.starts_with(&root) {
            return Err(invalid(
                "Managed asset points outside the project directory",
            ));
        }
        let audio = read_asset(&canonical, asset, cancel, true)?.unwrap();
        if revisions
            .insert(asset.revision_id.clone(), Arc::new(audio))
            .is_some()
        {
            return Err(invalid("Duplicate asset revision"));
        }
        roots.insert(root);
        progress(((index + 1) * 90 / count) as u8);
    }
    validate_payload(&file.payload, &revisions)?;
    for doc in &mut file.payload.documents {
        let audio = &revisions[&doc.revision_id];
        doc.peak_level = audio.peak_level;
        doc.overloaded_samples = audio.overloaded_samples;
        doc.refresh_dirty(file.payload.arrangement.effect_tempo);
    }
    file.payload.arrangement.saved_revision_id = Some(file.payload.arrangement.revision_id.clone());
    file.payload.arrangement.dirty = false;
    let revision_id = StateKey::new(&file.payload.documents, &file.payload.arrangement).revision();
    let summary = ProjectSummary {
        path: unicode(&path)?,
        revision_id,
        schema_version: SCHEMA_VERSION,
        documents: file.payload.documents.len(),
        tracks: file.payload.arrangement.tracks.len(),
        clips: file
            .payload
            .arrangement
            .tracks
            .iter()
            .map(|t| t.clips.len())
            .sum(),
        assets: revisions.len(),
        assets_written: 0,
        assets_reused: revisions.len(),
        managed_assets_path: roots
            .iter()
            .next()
            .map_or_else(String::new, |path| path.to_string_lossy().into_owned()),
    };
    progress(100);
    let asset_roots = roots
        .iter()
        .map(|path| unicode(path))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(LoadedProject {
        payload: file.payload,
        revisions,
        summary,
        asset_roots,
    })
}
fn validate_payload(
    payload: &Payload,
    revisions: &BTreeMap<String, Arc<Audio>>,
) -> Result<(), CommandError> {
    if payload.processing_policy != super::processing_policy() {
        return Err(invalid("Unsupported DSP or tail policy"));
    }
    let mut ids = BTreeSet::new();
    for doc in &payload.documents {
        canonical_id(&doc.id)?;
        canonical_id(&doc.revision_id)?;
        doc.processing
            .validate(doc.frames)
            .map_err(|error| invalid(&error.message))?;
        doc.saved_processing
            .validate(u64::MAX)
            .map_err(|e| invalid(&e.message))?;
        if !doc.saved_effect_tempo.is_finite()
            || !(20.0..=400.0).contains(&doc.saved_effect_tempo)
            || doc.save_channels.is_some_and(|c| !(1..=2).contains(&c))
        {
            return Err(invalid("Invalid saved output baseline"));
        }
        if !ids.insert(doc.id.clone()) {
            return Err(invalid("Duplicate document ID"));
        }
        let audio = revisions
            .get(&doc.revision_id)
            .ok_or_else(|| invalid("Document references a missing revision"))?;
        if doc.sample_rate != audio.rate
            || doc.channels != audio.channels
            || doc.frames != audio.frames()
            || doc.cursor > doc.frames
            || doc
                .selection
                .as_ref()
                .is_some_and(|r| r.start >= r.end || r.end > doc.frames)
        {
            return Err(invalid("Invalid document metadata or navigation"));
        }
        if let Some(saved) = &doc.saved_revision_id {
            canonical_id(saved)?;
        }
        if let Some(format) = &doc.save_format {
            format
                .output_rate(doc.sample_rate, doc.save_sample_rate)
                .map_err(|error| invalid(&error.message))?;
        }
    }
    if payload
        .active_document_id
        .as_ref()
        .is_some_and(|id| !ids.contains(id))
    {
        return Err(invalid("Active document is missing"));
    }
    let arr = &payload.arrangement;
    canonical_id(&arr.id)?;
    canonical_id(&arr.revision_id)?;
    if !arr.effect_tempo.is_finite()
        || !(20.0..=400.0).contains(&arr.effect_tempo)
        || arr.sample_rate != 48000
        || !arr.master_gain_db.is_finite()
        || !(-120.0..=24.0).contains(&arr.master_gain_db)
        || arr.cursor > crate::arrangement::MAX_TIMELINE_FRAME
        || arr
            .selection
            .as_ref()
            .is_some_and(|r| r.start >= r.end || r.end > crate::arrangement::MAX_TIMELINE_FRAME)
    {
        return Err(invalid("Invalid arrangement rate, controls or navigation"));
    }
    let mut track_ids = BTreeSet::new();
    let mut clip_ids = BTreeSet::new();
    let mut orders = BTreeSet::new();
    for track in &arr.tracks {
        canonical_id(&track.id)?;
        if !track_ids.insert(&track.id)
            || track.name.trim().is_empty()
            || track.name.chars().count() > 128
            || !track.gain_db.is_finite()
            || !(-120.0..=24.0).contains(&track.gain_db)
            || !track.pan.is_finite()
            || !(-1.0..=1.0).contains(&track.pan)
        {
            return Err(invalid("Invalid or duplicate track"));
        }
        for clip in &track.clips {
            canonical_id(&clip.id)?;
            canonical_id(&clip.document_id)?;
            canonical_id(&clip.edit_revision_id)?;
            clip.processing
                .validate(clip.source_end.saturating_sub(clip.source_start))
                .map_err(|error| invalid(&error.message))?;
            if clip.linked && !ids.contains(&clip.document_id) {
                return Err(invalid("Linked clip document is missing"));
            }
            if !clip_ids.insert(&clip.id)
                || !orders.insert(clip.insertion_order)
                || clip.insertion_order >= arr.next_clip_order
            {
                return Err(invalid("Invalid clip identity or insertion order"));
            }
            let source = revisions
                .get(&clip.revision_id)
                .ok_or_else(|| invalid("Clip source revision is missing"))?;
            let bounce = clip
                .bounce
                .as_ref()
                .map(|b| {
                    canonical_id(&b.audio_revision_id)?;
                    canonical_id(&b.source_revision_id)?;
                    let rendered = revisions
                        .get(&b.audio_revision_id)
                        .ok_or_else(|| invalid("Bounce asset is missing"))?;
                    if b.source_start >= b.source_end
                        || b.metadata.source_body_frames != b.source_end - b.source_start
                        || b.metadata.audio_revision_id != b.audio_revision_id
                        || b.metadata.render_identity != b.render_identity
                        || b.metadata.sample_rate != rendered.rate
                        || b.metadata.channels != rendered.channels
                        || b.metadata
                            .source_body_frames
                            .checked_add(b.metadata.tail_frames)
                            != Some(rendered.frames())
                        || b.metadata.tail_frames > rendered.rate as u64 * 60
                        || b.metadata.latency_frames > rendered.rate as u64
                    {
                        return Err(invalid("Invalid bounce metadata or dimensions"));
                    }
                    if clip.linked {
                        let doc = payload
                            .documents
                            .iter()
                            .find(|d| d.id == clip.document_id)
                            .unwrap();
                        let expected = checksum(&(
                            doc.processing.identity(&doc.revision_id, arr.effect_tempo),
                            clip.source_start,
                            clip.source_end,
                        ));
                        if b.source_revision_id != doc.revision_id
                            || b.source_start != clip.source_start
                            || b.source_end != clip.source_end
                            || b.render_identity != expected
                        {
                            return Err(invalid("Linked bounce is stale"));
                        }
                    }
                    Ok(rendered)
                })
                .transpose()?;
            let expected_source_frames = if !clip.linked
                && clip
                    .bounce
                    .as_ref()
                    .is_some_and(|b| b.audio_revision_id == clip.revision_id)
            {
                clip.bounce.as_ref().unwrap().metadata.source_body_frames
            } else {
                source.frames()
            };
            let source_channels = bounce.map_or(source.channels, |b| b.channels);
            if clip.source_start >= clip.source_end
                || clip.source_end > expected_source_frames
                || clip.source_frames != expected_source_frames
                || clip.source_rate != source.rate
                || clip.channels != source_channels
                || clip.frames
                    != audio::resample::frame_count(
                        clip.source_end - clip.source_start,
                        source.rate,
                        arr.sample_rate,
                    )
                || clip.tail_frames > arr.sample_rate as u64 * 60
                || clip.source_range_intent.as_ref().is_some_and(|r| {
                    !clip.linked
                        || r.start >= r.end
                        || r.start.min(source.frames()) != clip.source_start
                        || r.end.min(source.frames()) != clip.source_end
                })
            {
                return Err(invalid("Clip source interval or format is invalid"));
            }
            if let Some(intent) = &clip.envelope_intent {
                intent.validate(u64::MAX).map_err(|e| invalid(&e.message))?;
            }
            if clip.effects.len() > 32 {
                return Err(invalid("Too many clip processing steps"));
            }
            for effect in &clip.effects {
                effect.validate().map_err(|error| invalid(&error.message))?;
            }
            if let Some(processed) = &clip.processed_revision_id {
                let audio = revisions
                    .get(processed)
                    .ok_or_else(|| invalid("Processed clip revision is missing"))?;
                if audio.rate != source.rate
                    || audio.channels != source.channels
                    || audio.frames() != clip.source_end - clip.source_start
                {
                    return Err(invalid("Processed clip dimensions are invalid"));
                }
            } else if !clip.effects.is_empty() {
                return Err(invalid("Clip effects have no prepared audio"));
            }
        }
    }
    if arr
        .selected_clip_ids
        .iter()
        .any(|id| !clip_ids.contains(id))
    {
        return Err(invalid("Selected clip is missing"));
    }
    arr.validate().map_err(|error| invalid(&error.message))?;
    Ok(())
}
fn canonical_id(id: &str) -> Result<(), CommandError> {
    if uuid::Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id) {
        Ok(())
    } else {
        Err(invalid("Project contains an invalid object/revision ID"))
    }
}
fn unicode(path: &Path) -> Result<String, CommandError> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Project path is not valid Unicode"))
}
fn invalid(message: &str) -> CommandError {
    CommandError::new(ErrorCode::InvalidProject, message)
}
