//! The approved six-dial Reverb editor, adapting the unchanged reference DSP.
use super::effect_panel::{Dial, dial};
use super::*;
use wavebob::{
    dsp::reverb,
    processing::{Instance, ParameterValue},
};

type Change = (String, ParameterValue, bool);

#[derive(Clone, Copy, Debug)]
pub(super) enum Control {
    Size,
    Decay,
    Predelay,
    Damping,
    Width,
    Mix,
}

impl Dial for Control {
    fn fraction(self, value: f64) -> f64 {
        let (min, max, _) = self.range();
        if matches!(self, Self::Decay | Self::Damping) {
            (value / min).ln() / (max / min).ln()
        } else {
            (value - min) / (max - min)
        }
        .clamp(0.0, 1.0)
    }
    fn at_fraction(self, fraction: f64) -> f64 {
        let (min, max, _) = self.range();
        let fraction = fraction.clamp(0.0, 1.0);
        if matches!(self, Self::Decay | Self::Damping) {
            min * (max / min).powf(fraction)
        } else {
            min + (max - min) * fraction
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::Size => "Size",
            Self::Decay => "Decay",
            Self::Predelay => "Pre-delay",
            Self::Damping => "Damping",
            Self::Width => "Width",
            Self::Mix => "Mix",
        }
    }
    fn range(self) -> (f64, f64, f64) {
        match self {
            Self::Size => (1.0, 100.0, 1.0),
            Self::Decay => (0.1, 20.0, 0.1),
            Self::Predelay => (0.0, 1000.0, 1000.0 / 384.0),
            Self::Damping => (0.5, 22.1, 0.1),
            Self::Width => (0.0, 200.0, 100.0 / 64.0),
            Self::Mix => (0.0, 100.0, 100.0 / 128.0),
        }
    }
    fn reset(self) -> f64 {
        match self {
            Self::Size => reverb::DEFAULTS[3] as f64,
            Self::Decay => reverb::DEFAULTS[5] as f64 / 10.0,
            Self::Predelay => 0.0,
            Self::Damping => reverb::DEFAULTS[6] as f64 / 10.0,
            Self::Width => 100.0,
            Self::Mix => 30.0,
        }
    }
    fn text(self, value: f64) -> String {
        match self {
            Self::Size | Self::Width | Self::Mix => format!("{value:.0}%"),
            Self::Decay => format!("{value:.1} s"),
            Self::Predelay => format!("{value:.0} ms"),
            Self::Damping if value >= 22.1 => "Off".into(),
            Self::Damping => format!("{value:.1} kHz"),
        }
    }
}

impl Control {
    pub(super) fn read(self, instance: &Instance, bpm: f64) -> f64 {
        let get = |id: &str| match instance.parameters[id] {
            ParameterValue::Integer(v) => v as f64,
            _ => unreachable!("Validated Reverb"),
        };
        match self {
            Self::Size => get("size"),
            Self::Decay => get("decay") / 10.0,
            Self::Damping => get("damping") / 10.0,
            Self::Predelay => {
                if instance.parameters["tempo_sync"] == ParameterValue::Boolean(true) {
                    get("predelay") / 192.0 * 60_000.0 / bpm
                } else {
                    get("predelay") * 1000.0 / 384.0
                }
            }
            Self::Width => 100.0 * (1.0 - get("separation") / 64.0),
            // Reading an older independent mix must never normalize its sound.
            Self::Mix => {
                let wet = (get("early") + get("wet")) * 0.5;
                let total = get("dry") + wet;
                if total > 0.0 {
                    100.0 * wet / total
                } else {
                    0.0
                }
            }
        }
    }
    pub(super) fn changes(self, value: f64) -> Vec<Change> {
        let integer = |id: &str, value: f64| {
            (
                id.into(),
                ParameterValue::Integer(value.round() as i32),
                false,
            )
        };
        match self {
            Self::Size => vec![integer("size", value)],
            Self::Decay => vec![integer("decay", value * 10.0)],
            Self::Damping => vec![integer("damping", value * 10.0)],
            Self::Predelay => vec![
                integer("predelay", value * 384.0 / 1000.0),
                ("tempo_sync".into(), ParameterValue::Boolean(false), false),
            ],
            Self::Width => vec![integer("separation", (1.0 - value / 100.0) * 64.0)],
            Self::Mix => {
                let wet = (value * 128.0 / 100.0).round();
                vec![
                    integer("dry", 128.0 - wet),
                    integer("early", wet),
                    integer("wet", wet),
                ]
            }
        }
    }
}

impl WaveBob {
    pub(super) fn reverb_controls(
        &mut self,
        ui: &mut egui::Ui,
        window: &EditorWindow,
        instance: &Instance,
        native: &mut effects::NativeEditor,
    ) {
        // Below the readable minimum, scrolling preserves usable exact controls.
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (canvas, scale, origin) = self.effect_panel(ui, window, instance, native);
                let point = |x, y| origin + Vec2::new(x, y) * scale;
                let painter = ui.painter_at(canvas);
                let bpm = self.host.snapshot().arrangement.effect_tempo;
                let (field, scene) = reverb_field::scene(
                    Rect::from_min_max(point(207.0, 70.0), point(643.0, 359.0)),
                    instance,
                );
                if let Some(frame) = &mut native.visualization {
                    frame.place(&painter);
                    if let Err(error) = frame.submit(
                        egui::Id::unique("reflections"),
                        visualization::revision(instance),
                        field,
                        field.expand(12.0).intersect(canvas),
                        scene,
                    ) {
                        self.error = Some(error.into());
                    }
                }
                for (control, x, y, radius) in [
                    (Control::Predelay, 100.0, 194.0, 46.0),
                    (Control::Width, 100.0, 393.0, 46.0),
                    (Control::Damping, 753.0, 194.0, 46.0),
                    (Control::Mix, 753.0, 393.0, 46.0),
                    (Control::Size, 326.0, 437.0, 51.0),
                    (Control::Decay, 524.0, 437.0, 51.0),
                ] {
                    // Manual placement must not advance the scroll area's cursor
                    // after the full canvas has already been allocated.
                    let mut control_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt(control.label())
                            .max_rect(canvas),
                    );
                    let mut value = control.read(instance, bpm);
                    let response = dial(
                        &mut control_ui,
                        point(x, y),
                        radius * scale,
                        scale,
                        control,
                        &mut value,
                    );
                    self.edit_native_parameters(
                        window,
                        &instance.id,
                        native,
                        control.changes(value),
                        &response,
                        ui.ctx(),
                    );
                }
            });
    }
}
