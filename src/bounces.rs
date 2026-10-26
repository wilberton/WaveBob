//! Immutable lossless document bounces and bounded cache, prepared away from audio/UI.
use crate::{
    CommandError, ErrorCode, audio::Audio, documents::FrameRange, processing::Processing,
    rendering::RenderMetadata,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, atomic::AtomicBool},
};
pub const CACHE_BYTES: u64 = 512 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Reference {
    pub render_identity: String,
    pub audio_revision_id: String,
    pub source_revision_id: String,
    pub source_start: u64,
    pub source_end: u64,
    pub metadata: RenderMetadata,
}
#[derive(Debug)]
pub(crate) struct Asset {
    pub reference: Reference,
    pub audio: Arc<Audio>,
}
pub(crate) type Cache = BTreeMap<String, Arc<Asset>>;
#[derive(Clone)]
pub(crate) struct DocumentInput {
    pub revision_id: String,
    pub audio: Arc<Audio>,
    pub state: Processing,
}
pub(crate) fn bytes(cache: &Cache) -> u64 {
    let mut seen = BTreeSet::new();
    cache
        .values()
        .filter(|a| seen.insert(a.audio.samples.as_ptr() as usize))
        .map(|a| a.audio.samples.len() as u64 * 4)
        .sum()
}
fn asset(
    doc: &DocumentInput,
    range: FrameRange,
    tempo: f64,
    cache: &mut Cache,
    cancel: &AtomicBool,
) -> Result<Arc<Asset>, CommandError> {
    let identity = doc.state.identity(&doc.revision_id, tempo);
    let key = crate::projects::checksum(&(&identity, range.start, range.end));
    if let Some(asset) = cache.get(&key) {
        return Ok(asset.clone());
    }
    crate::exports::check_cancel(cancel)?;
    let (audio, mut metadata) = if range.start == 0
        && range.end == doc.audio.frames()
        && doc.state.same_sound(&Processing::default())
    {
        (
            doc.audio.clone(),
            RenderMetadata {
                source_body_frames: doc.audio.frames(),
                sample_rate: doc.audio.rate,
                channels: doc.audio.channels,
                audio_revision_id: doc.revision_id.clone(),
                ..Default::default()
            },
        )
    } else {
        let (audio, metadata) = crate::rendering::offline_document_range(
            &doc.audio,
            range.clone(),
            &doc.state,
            tempo,
            cancel,
        )?;
        (Arc::new(audio), metadata)
    };
    metadata.render_identity = key.clone();
    let a = Arc::new(Asset {
        reference: Reference {
            render_identity: key.clone(),
            audio_revision_id: metadata.audio_revision_id.clone(),
            source_revision_id: doc.revision_id.clone(),
            source_start: range.start,
            source_end: range.end,
            metadata,
        },
        audio,
    });
    let extra = a.audio.samples.len() as u64 * 4;
    while bytes(cache).saturating_add(extra) > CACHE_BYTES {
        let removable = cache
            .iter()
            .find(|(_, a)| Arc::strong_count(&a.audio) == 1)
            .map(|(k, _)| k.clone());
        if let Some(key) = removable {
            cache.remove(&key);
        } else {
            return Err(crate::rendering::limit(
                "Referenced document bounces exceed the 512 MiB cache budget",
            ));
        }
    }
    cache.insert(key, a.clone());
    Ok(a)
}
/// Build all candidate body geometry before rendering anything. On failure the
/// caller retains every prior source and editable placement unchanged.
pub(crate) fn prepare_inputs(
    arrangement: &crate::arrangement::Arrangement,
    mut inputs: Vec<crate::audio::mix::Input>,
    mut cache: Cache,
    cancel: &AtomicBool,
) -> Result<
    (
        crate::arrangement::Arrangement,
        Vec<crate::audio::mix::Input>,
        Cache,
    ),
    CommandError,
> {
    let mut candidate = arrangement.clone();
    let mut affected = Vec::new();
    let mut empty = Vec::new();
    for input in &mut inputs {
        let Some(doc) = &input.document else {
            continue;
        };
        let n = doc.audio.frames();
        let clip = &mut input.clip;
        let requested = clip.source_range_intent.clone().unwrap_or(FrameRange {
            start: clip.source_start,
            end: clip.source_end,
        });
        let start = if clip.full_document {
            0
        } else {
            requested.start.min(n)
        };
        let end = if clip.full_document {
            n
        } else {
            requested.end.min(n)
        };
        clip.source_range_intent =
            if !clip.full_document && (requested.start != start || requested.end != end) {
                Some(requested)
            } else {
                None
            };
        if start >= end {
            empty.push(clip.id.clone());
            continue;
        }
        affected.push(clip.id.clone());
        clip.source_start = start;
        clip.source_end = end;
        clip.source_frames = n;
        clip.source_rate = doc.audio.rate;
        clip.channels = doc.audio.channels;
        clip.revision_id = doc.revision_id.clone();
        clip.frames =
            crate::audio::resample::frame_count(end - start, doc.audio.rate, candidate.sample_rate);
        clip.clamp_source_envelope(end - start);
        *candidate
            .tracks
            .iter_mut()
            .flat_map(|t| &mut t.clips)
            .find(|c| c.id == clip.id)
            .unwrap() = clip.clone();
    }
    if !empty.is_empty() {
        return Err(CommandError {
            code: ErrorCode::InvalidParams,
            message: "Linked refresh would leave empty clip intervals".into(),
            details: Some(serde_json::json!({"affected_clip_ids":empty})),
        });
    }
    if let Err(mut error) = candidate.validate() {
        error.message = format!("Linked refresh blocked: {}", error.message);
        error.details = Some(serde_json::json!({"affected_clip_ids":affected}));
        return Err(error);
    }
    for input in &mut inputs {
        crate::exports::check_cancel(cancel)?;
        if let Some(doc) = &input.document {
            let old = input
                .clip
                .bounce
                .as_ref()
                .map(|b| b.render_identity.clone());
            let a = asset(
                doc,
                FrameRange {
                    start: input.clip.source_start,
                    end: input.clip.source_end,
                },
                candidate.effect_tempo,
                &mut cache,
                cancel,
            )?;
            input.clip.channels = a.audio.channels;
            input.clip.bounce = Some(a.reference.clone());
            if old.as_ref() != Some(&a.reference.render_identity) {
                input.clip.edit_revision_id = uuid::Uuid::new_v4().to_string();
            }
            input.audio = a.audio.clone();
            input.range = FrameRange {
                start: 0,
                end: input.audio.frames(),
            };
        }
        *candidate
            .tracks
            .iter_mut()
            .flat_map(|t| &mut t.clips)
            .find(|c| c.id == input.clip.id)
            .unwrap() = input.clip.clone();
    }
    Ok((candidate, inputs, cache))
}
