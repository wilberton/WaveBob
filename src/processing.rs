//! Serializable, owner-scoped live processing. DSP availability is explicit.
use crate::{CommandError, ErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const DSP_VERSION: u32 = 1;
pub const MAX_EFFECTS: usize = 32;
pub const REFERENCE_COMMIT: &str = "0af860dfd31c7ee9cbf8af6747617e031a1a7742";

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Deserialize, Serialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "id",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Owner {
    Document(String),
    Clip(String),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    /// None is explicit mute, never an infinity encoded as JSON.
    pub gain_db: Option<f64>,
    pub fade_in_frames: u64,
    pub fade_out_frames: u64,
}
impl Default for Envelope {
    fn default() -> Self {
        Self::unity()
    }
}
impl Envelope {
    pub fn unity() -> Self {
        Self {
            gain_db: Some(0.0),
            fade_in_frames: 0,
            fade_out_frames: 0,
        }
    }
    pub fn validate(&self, frames: u64) -> Result<(), CommandError> {
        if self
            .gain_db
            .is_some_and(|v| !v.is_finite() || !(-120.0..=60.0).contains(&v))
            || self.fade_in_frames > frames
            || self.fade_out_frames > frames
        {
            return Err(invalid(
                "Gain must be finite -120..60 dB or null (mute); fades must fit the body",
            ));
        }
        Ok(())
    }
    pub fn amplitude(&self, frame: u64, frames: u64) -> f64 {
        if frame >= frames {
            return 0.0;
        }
        let ramp = |n: u64, distance: u64| {
            if n <= 1 {
                1.0
            } else {
                (distance as f64 / (n - 1) as f64).min(1.0)
            }
        };
        self.gain_db.map_or(0.0, |db| 10.0_f64.powf(db / 20.0))
            * ramp(self.fade_in_frames, frame)
            * ramp(self.fade_out_frames, frames - 1 - frame)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    SoftClipper,
    Reverb,
    ParametricEq,
    FastDistortion,
    Delay,
    Chorus,
    Limiter,
    Balance,
    StereoEnhancer,
    StereoShaper,
    TransientProcessor,
}
impl EffectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::SoftClipper => "Soft Clipper",
            Self::Reverb => "Reverb",
            Self::ParametricEq => "Parametric EQ",
            Self::FastDistortion => "Fast Distortion",
            Self::Delay => "Delay",
            Self::Chorus => "Chorus",
            Self::Limiter => "Limiter",
            Self::Balance => "Balance",
            Self::StereoEnhancer => "Stereo Enhancer",
            Self::StereoShaper => "Stereo Shaper",
            Self::TransientProcessor => "Transient Processor",
        }
    }

    pub const ALL: [Self; 11] = [
        Self::SoftClipper,
        Self::Reverb,
        Self::ParametricEq,
        Self::FastDistortion,
        Self::Delay,
        Self::Chorus,
        Self::Limiter,
        Self::Balance,
        Self::StereoEnhancer,
        Self::StereoShaper,
        Self::TransientProcessor,
    ];
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ParameterValue {
    Boolean(bool),
    Integer(i32),
    Number(f64),
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct ParameterInfo {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub value_type: String,
    pub default: ParameterValue,
    pub minimum: f64,
    pub maximum: f64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct EffectInfo {
    pub kind: EffectKind,
    pub available: bool,
    pub parameters: Vec<ParameterInfo>,
}
pub fn catalogue() -> Vec<EffectInfo> {
    EffectKind::ALL
        .into_iter()
        .map(|kind| EffectInfo {
            kind,
            available: false,
            parameters: Vec::new(),
        })
        .collect()
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Instance {
    pub id: String,
    pub kind: EffectKind,
    pub bypassed: bool,
    pub parameters: BTreeMap<String, ParameterValue>,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Processing {
    pub edit_revision_id: String,
    pub envelope: Envelope,
    pub bypassed: bool,
    pub effects: Vec<Instance>,
}
impl Default for Processing {
    fn default() -> Self {
        Self {
            edit_revision_id: uuid::Uuid::new_v4().to_string(),
            envelope: Envelope::unity(),
            bypassed: false,
            effects: Vec::new(),
        }
    }
}
impl Processing {
    pub fn validate(&self, frames: u64) -> Result<(), CommandError> {
        self.envelope.validate(frames)?;
        if uuid::Uuid::parse_str(&self.edit_revision_id).is_err() {
            return Err(invalid("Invalid processing revision identity"));
        }
        if self.effects.len() > MAX_EFFECTS {
            return Err(CommandError::new(
                ErrorCode::ResourceLimit,
                "At most 32 effects per owner",
            ));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut check = self.clone();
        for effect in &self.effects {
            if uuid::Uuid::parse_str(&effect.id).is_err() || !seen.insert(&effect.id) {
                return Err(invalid("Invalid or duplicate effect identity"));
            }
            let info = catalogue()
                .into_iter()
                .find(|i| i.kind == effect.kind)
                .unwrap();
            if !info.available {
                return Err(CommandError::new(
                    ErrorCode::MissingCapability,
                    "Processor is not implemented yet",
                ));
            }
            if effect.parameters.len() != info.parameters.len() {
                return Err(invalid("Effect parameter set is incomplete"));
            }
            for (id, value) in &effect.parameters {
                check.apply(
                    Action::SetParameter {
                        effect_id: effect.id.clone(),
                        parameter_id: id.clone(),
                        value: value.clone(),
                    },
                    frames,
                )?;
            }
        }
        Ok(())
    }
    pub fn identity(&self, source: &str, tempo: f64) -> String {
        crate::projects::checksum(
            &serde_json::json!({"source":source,"envelope":self.envelope,"bypassed":self.bypassed,"effects":self.effects,"tempo":tempo,"tail_policy":{"threshold_db":-100,"hold_ms":100,"cap_seconds":60,"taper_ms":10},"dsp_version":DSP_VERSION}),
        )
    }
    pub fn same_sound(&self, other: &Self) -> bool {
        self.envelope == other.envelope
            && self.bypassed == other.bypassed
            && self.effects == other.effects
    }
    pub fn apply(&mut self, action: Action, frames: u64) -> Result<(), CommandError> {
        match action {
            Action::SetEnvelope { envelope } => {
                envelope.validate(frames)?;
                self.envelope = envelope;
            }
            Action::Add { kind } => {
                let info = catalogue().into_iter().find(|i| i.kind == kind).unwrap();
                if !info.available {
                    return Err(CommandError::new(
                        ErrorCode::MissingCapability,
                        "Processor is not implemented yet",
                    ));
                }
                if self.effects.len() >= MAX_EFFECTS {
                    return Err(CommandError::new(
                        ErrorCode::ResourceLimit,
                        "At most 32 effects per owner",
                    ));
                }
                self.effects.push(Instance {
                    id: uuid::Uuid::new_v4().to_string(),
                    kind,
                    bypassed: false,
                    parameters: info
                        .parameters
                        .into_iter()
                        .map(|p| (p.id, p.default))
                        .collect(),
                });
            }
            Action::Remove { effect_id } => {
                let index = self.index(&effect_id)?;
                self.effects.remove(index);
            }
            Action::Move { effect_id, index } => {
                let old = self.index(&effect_id)?;
                if index >= self.effects.len() {
                    return Err(invalid("Effect index outside stack"));
                }
                let effect = self.effects.remove(old);
                self.effects.insert(index, effect);
            }
            Action::Bypass {
                effect_id,
                bypassed,
            } => {
                let index = self.index(&effect_id)?;
                self.effects[index].bypassed = bypassed;
            }
            Action::BypassAll { bypassed } => self.bypassed = bypassed,
            Action::SetParameter {
                effect_id,
                parameter_id,
                value,
            } => {
                let index = self.index(&effect_id)?;
                let info = catalogue()
                    .into_iter()
                    .find(|p| p.kind == self.effects[index].kind)
                    .unwrap();
                let param = info
                    .parameters
                    .iter()
                    .find(|p| p.id == parameter_id)
                    .ok_or_else(|| invalid("Unknown parameter"))?;
                let number = match (&value, param.value_type.as_str()) {
                    (ParameterValue::Number(v), "number") => *v,
                    (ParameterValue::Integer(v), "integer") => *v as f64,
                    (ParameterValue::Boolean(v), "boolean") => u8::from(*v) as f64,
                    _ => return Err(invalid("Parameter type mismatch")),
                };
                if !number.is_finite() || number < param.minimum || number > param.maximum {
                    return Err(invalid("Parameter outside supported range"));
                }
                self.effects[index].parameters.insert(parameter_id, value);
            }
        }
        Ok(())
    }
    fn index(&self, id: &str) -> Result<usize, CommandError> {
        self.effects
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| CommandError::new(ErrorCode::NotFound, "Effect not found"))
    }
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    SetEnvelope {
        envelope: Envelope,
    },
    Add {
        kind: EffectKind,
    },
    Remove {
        effect_id: String,
    },
    Move {
        effect_id: String,
        index: usize,
    },
    Bypass {
        effect_id: String,
        bypassed: bool,
    },
    BypassAll {
        bypassed: bool,
    },
    SetParameter {
        effect_id: String,
        parameter_id: String,
        value: ParameterValue,
    },
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OwnerParams {
    pub owner: Owner,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditParams {
    pub owner: Owner,
    pub expected_edit_revision: String,
    pub change: Action,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BeginParams {
    pub owner: Owner,
    pub expected_edit_revision: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PreviewParams {
    pub preview_id: String,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateParams {
    pub preview_id: String,
    pub change: Action,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TempoParams {
    pub bpm: f64,
}
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct Snapshot {
    pub owner: Owner,
    pub committed: Processing,
    pub preview: Option<Processing>,
    pub preview_id: Option<String>,
    pub render_identity: String,
    pub effect_tempo: f64,
    pub body_frames: u64,
}
pub(crate) fn invalid(message: &str) -> CommandError {
    CommandError::new(ErrorCode::InvalidParams, message)
}
