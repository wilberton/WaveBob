use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ViewMode {
    #[default]
    Waveform,
    Multitrack,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct PresentationSnapshot {
    pub revision: u64,
    pub mode: ViewMode,
    pub layout: PanelLayout,
    pub preferences: PreferenceStatus,
}

pub const MIN_PANEL_WIDTH: f32 = 180.0;
pub const MIN_CENTER_WIDTH: f32 = 400.0;
pub const COLLAPSED_WIDTH: f32 = 24.0;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SidePanelLayout {
    pub visible: bool,
    pub width: f32,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelLayout {
    pub left: SidePanelLayout,
    pub right: SidePanelLayout,
    /// Fraction of the left column reserved for open documents.
    pub library_fraction: f32,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            left: SidePanelLayout {
                visible: true,
                width: 250.0,
            },
            right: SidePanelLayout {
                visible: true,
                width: 240.0,
            },
            library_fraction: 0.27,
        }
    }
}

impl PanelLayout {
    pub fn validate(&self) -> Result<(), crate::CommandError> {
        if [self.left.width, self.right.width]
            .iter()
            .any(|width| !width.is_finite() || *width < MIN_PANEL_WIDTH)
            || !self.library_fraction.is_finite()
            || !(0.15..=0.85).contains(&self.library_fraction)
        {
            return Err(crate::CommandError::new(
                crate::ErrorCode::InvalidParams,
                "Panel widths must be finite and at least 180 logical pixels; library_fraction must be between 0.15 and 0.85",
            ));
        }
        Ok(())
    }

    /// Clamp rendered widths without overwriting the user's expanded sizes.
    pub fn effective_widths(&self, window_width: f32) -> [f32; 2] {
        let panels = [&self.left, &self.right];
        let visible = panels.iter().filter(|panel| panel.visible).count() as f32;
        let budget = (window_width - MIN_CENTER_WIDTH - (2.0 - visible) * COLLAPSED_WIDTH).max(0.0);
        let minimum = visible * MIN_PANEL_WIDTH;
        let extra = panels
            .iter()
            .filter(|panel| panel.visible)
            .map(|panel| panel.width - MIN_PANEL_WIDTH)
            .sum::<f32>();
        let factor = if extra > 0.0 {
            ((budget - minimum) / extra).clamp(0.0, 1.0)
        } else {
            0.0
        };
        panels.map(|panel| {
            if panel.visible {
                if budget < minimum {
                    budget / visible.max(1.0)
                } else {
                    MIN_PANEL_WIDTH + (panel.width - MIN_PANEL_WIDTH) * factor
                }
            } else {
                COLLAPSED_WIDTH
            }
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PreferenceStatus {
    #[default]
    MemoryOnly,
    Saved,
    Saving,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetLayoutParams {
    #[schemars(range(min = 180))]
    pub left_width: Option<f32>,
    #[schemars(range(min = 180))]
    pub right_width: Option<f32>,
    pub left_visible: Option<bool>,
    pub right_visible: Option<bool>,
    #[schemars(range(min = 0.15, max = 0.85))]
    pub library_fraction: Option<f32>,
    pub expected_presentation_revision: Option<u64>,
}

impl SetLayoutParams {
    pub fn apply(&self, layout: &PanelLayout) -> Result<PanelLayout, crate::CommandError> {
        let mut layout = layout.clone();
        if let Some(width) = self.left_width {
            layout.left.width = width;
        }
        if let Some(width) = self.right_width {
            layout.right.width = width;
        }
        if let Some(visible) = self.left_visible {
            layout.left.visible = visible;
        }
        if let Some(visible) = self.right_visible {
            layout.right.visible = visible;
        }
        if let Some(fraction) = self.library_fraction {
            layout.library_fraction = fraction;
        }
        layout.validate()?;
        Ok(layout)
    }
}

#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresentationRevisionParams {
    pub expected_presentation_revision: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetModeParams {
    pub mode: ViewMode,
    pub expected_presentation_revision: Option<u64>,
}

pub fn default_timeout_ms() -> u64 {
    10_000
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RenderParams {
    pub after_revision: Option<u64>,
    pub after_presentation_revision: Option<u64>,
    #[serde(default = "default_timeout_ms")]
    #[schemars(range(min = 1, max = 60_000))]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CaptureParams {
    pub path: Option<String>,
    #[serde(default)]
    pub overwrite: bool,
    pub after_revision: Option<u64>,
    pub after_presentation_revision: Option<u64>,
    #[serde(default = "default_timeout_ms")]
    #[schemars(range(min = 1, max = 60_000))]
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct RenderMetadata {
    pub session_id: String,
    pub rendered_revision: u64,
    pub presentation_revision: u64,
    pub mode: ViewMode,
    pub frame_number: u64,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub pending_resources: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct CaptureResult {
    pub path: String,
    pub mime_type: String,
    pub metadata: RenderMetadata,
}
