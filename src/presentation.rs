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
    #[serde(default)]
    pub viewports: std::collections::BTreeMap<String, crate::documents::FrameRange>,
    #[serde(default)]
    pub effect: crate::effects::Effect,
    #[serde(default)]
    pub close_requested: bool,
}
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuitParams {
    #[serde(default)]
    pub discard_changes: bool,
    pub expected_presentation_revision: Option<u64>,
}
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetEffectParams {
    pub effect: crate::effects::Effect,
    pub expected_presentation_revision: Option<u64>,
}

impl PresentationSnapshot {
    pub fn visible_interval(
        &self,
        document: &crate::documents::DocumentInfo,
    ) -> crate::documents::FrameRange {
        let Some(view) = self.viewports.get(&document.id) else {
            return crate::documents::FrameRange {
                start: 0,
                end: document.frames,
            };
        };
        if document.frames == 0 {
            return crate::documents::FrameRange { start: 0, end: 0 };
        }
        if view.start == view.end {
            return crate::documents::FrameRange {
                start: 0,
                end: document.frames,
            };
        }
        let end = view.end.min(document.frames).max(1);
        crate::documents::FrameRange {
            start: view.start.min(end - 1),
            end,
        }
    }
}

#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetViewportParams {
    pub document_id: String,
    pub start_frame: u64,
    pub end_frame: u64,
    pub expected_presentation_revision: Option<u64>,
}

pub fn pan(
    view: &crate::documents::FrameRange,
    delta: f64,
    total: u64,
) -> crate::documents::FrameRange {
    let span = (view.end - view.start).min(total);
    let start = (view.start as f64 + delta)
        .round()
        .clamp(0.0, (total - span) as f64) as u64;
    crate::documents::FrameRange {
        start,
        end: start + span,
    }
}
pub fn zoom(
    view: &crate::documents::FrameRange,
    factor: f64,
    anchor: f64,
    total: u64,
) -> crate::documents::FrameRange {
    if total == 0 || !factor.is_finite() || factor <= 0.0 {
        return view.clone();
    }
    let anchor = anchor.clamp(0.0, 1.0);
    let span = ((view.end - view.start) as f64 / factor)
        .round()
        .clamp(1.0, total as f64) as u64;
    let frame = view.start as f64 + anchor * (view.end - view.start) as f64;
    let start = (frame - anchor * span as f64)
        .round()
        .clamp(0.0, (total - span) as f64) as u64;
    crate::documents::FrameRange {
        start,
        end: start + span,
    }
}

pub const MIN_PANEL_WIDTH: f32 = 180.0;
pub const MIN_CENTER_WIDTH: f32 = 400.0;
pub const COLLAPSED_WIDTH: f32 = 24.0;

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SidePanelLayout {
    pub visible: bool,
    pub width: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PanelLayout {
    pub left: SidePanelLayout,
    pub right: SidePanelLayout,
    /// Fraction of the left column reserved for open documents.
    pub library_fraction: f64,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            left: SidePanelLayout {
                visible: true,
                width: 240.0,
            },
            right: SidePanelLayout {
                visible: true,
                width: 200.0,
            },
            library_fraction: 0.18,
        }
    }
}

impl PanelLayout {
    pub fn validate(&self) -> Result<(), crate::CommandError> {
        if [self.left.width, self.right.width]
            .iter()
            .any(|width| !width.is_finite() || *width < MIN_PANEL_WIDTH as f64)
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
        let visible = panels.iter().filter(|panel| panel.visible).count() as f64;
        let min_width = MIN_PANEL_WIDTH as f64;
        let budget = (window_width as f64
            - MIN_CENTER_WIDTH as f64
            - (2.0 - visible) * COLLAPSED_WIDTH as f64)
            .max(0.0);
        let minimum = visible * min_width;
        let extra = panels
            .iter()
            .filter(|panel| panel.visible)
            .map(|panel| panel.width - min_width)
            .sum::<f64>();
        let factor = if extra > 0.0 {
            ((budget - minimum) / extra).clamp(0.0, 1.0)
        } else {
            0.0
        };
        panels.map(|panel| {
            (if panel.visible {
                if budget < minimum {
                    budget / visible.max(1.0)
                } else {
                    min_width + (panel.width - min_width) * factor
                }
            } else {
                COLLAPSED_WIDTH as f64
            }) as f32
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
    pub left_width: Option<f64>,
    #[schemars(range(min = 180))]
    pub right_width: Option<f64>,
    pub left_visible: Option<bool>,
    pub right_visible: Option<bool>,
    #[schemars(range(min = 0.15, max = 0.85))]
    pub library_fraction: Option<f64>,
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
