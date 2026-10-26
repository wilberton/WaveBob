//! Real OpenGL fixture/capture and bounded performance probe, not a second renderer.
#[path = "../src/app/reverb_field.rs"]
mod reverb_field;
#[path = "../src/app/visualization/mod.rs"]
mod visualization;
use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use eframe::glow::{self, HasContext};
use std::sync::Mutex;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use visualization::{Emission, Frame, Renderer, Scene};
#[derive(Default)]
struct GpuTiming {
    query: Option<glow::Query>,
    samples: Vec<f64>,
}
struct Review {
    renderer: Renderer,
    frame: u64,
    done: Arc<AtomicBool>,
    directory: PathBuf,
    failure: bool,
    mode: String,
    frame_cpu: Vec<f64>,
    build_cpu: Vec<f64>,
    curve: wavebob::dsp::fast_distortion::Curve,
    gpu: Arc<Mutex<GpuTiming>>,
    captures: Arc<Mutex<Vec<f64>>>,
}
impl eframe::App for Review {
    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if let Some(gl) = gl {
            self.renderer.destroy(gl);
            if self.directory.exists() {
                std::fs::write(
                    self.directory.join("closed-resources.json"),
                    serde_json::to_vec_pretty(&self.renderer.resources()).unwrap(),
                )
                .unwrap();
            }
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, engine: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(cpu) = engine.info().cpu_usage {
            self.frame_cpu.push(cpu as f64 * 1000.0);
        }
        if self.done.load(Ordering::Acquire) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ui, |ui| {
                let bounds = ui.max_rect();
                let started = std::time::Instant::now();
                let measured = self.mode.starts_with("reverb")
                    || self.mode.starts_with("distortion")
                    || self.mode == "dense";
                if measured {
                    let timing = self.gpu.clone();
                    ui.painter().add(egui::Shape::Callback(egui::PaintCallback {
                        rect: bounds,
                        callback: Arc::new(eframe::egui_glow::CallbackFn::new(
                            move |_, p| unsafe {
                                let gl = p.gl();
                                let q = gl.create_query().unwrap();
                                gl.begin_query(glow::TIME_ELAPSED, q);
                                timing.lock().unwrap().query = Some(q);
                            },
                        )),
                    }));
                }
                let mut frame = Frame::new(ctx.viewport_id(), bounds, ctx.pixels_per_point());
                if self.mode=="pressure" { frame.viewport=egui::ViewportId::from_hash_of(("pressure",self.frame%20)); }
                // The scene revision stays fixed while a parent viewport clip
                // moves by half a logical pixel inside the same target crop.
                let painter=if self.mode=="clip" && self.frame>=60 {
                    ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(if self.frame<120{424.25}else{424.75},115.5),Pos2::new(840.0,325.25)))
                }else{ui.painter().clone()};
                frame.place(&painter);
                if self.mode == "recovery" && self.frame == 75 {
                    self.renderer.retire(ctx.viewport_id());
                }
                if self.mode.starts_with("reverb") {
                    let mut instance = reverb_instance();
                    if self.mode.ends_with("dynamic") {
                        instance.parameters.insert(
                            "size".into(),
                            wavebob::processing::ParameterValue::Integer(
                                40 + (self.frame % 50) as i32,
                            ),
                        );
                    }
                    let (clip, scene) = reverb_field::scene(
                        Rect::from_min_max(Pos2::new(207.0, 70.0), Pos2::new(643.0, 359.0)),
                        &instance,
                    );
                    frame
                        .submit(
                            egui::Id::unique("pilot"),
                            visualization::revision(&instance),
                            clip,
                            clip.expand(12.0),
                            scene,
                        )
                        .unwrap();
                } else if self.mode.starts_with("distortion") {
                    let mut scene = Scene::default();
                    let plot = Rect::from_min_max(Pos2::new(242.0, 78.0), Pos2::new(623.0, 402.0));
                    let dynamic = self.mode.ends_with("dynamic");
                    let mut values = wavebob::dsp::fast_distortion::DEFAULTS;
                    values[1] = 2 + (self.frame % 8) as i32;
                    let changed =
                        dynamic.then(|| wavebob::dsp::fast_distortion::Curve::new(values).unwrap());
                    let curve = changed.as_ref().unwrap_or(&self.curve);
                    let limit = curve.sample(1.0).abs().max(1.0).ceil();
                    let points: Vec<_> = (0..=512)
                        .map(|i| {
                            let x = -1.0 + i as f32 / 256.0;
                            Pos2::new(
                                plot.left() + (x + 1.0) * plot.width() * 0.5,
                                plot.center().y - curve.sample(x) / limit * plot.height() * 0.5,
                            )
                        })
                        .collect();
                    scene.path(
                        points.clone(),
                        Stroke::new(1.8, Color32::from_rgb(0, 153, 255)),
                        Some(Emission::new(Color32::from_rgb(0, 153, 255), 2.0)),
                    );
                    scene.path(
                        points,
                        Stroke::new(0.55, Color32::from_rgb(143, 224, 255)),
                        None,
                    );
                    frame
                        .submit(
                            egui::Id::unique("pilot"),
                            if dynamic { self.frame + 1 } else { 1 },
                            plot,
                            plot.expand(12.0),
                            scene,
                        )
                        .unwrap();
                } else if self.mode == "dense" {
                    for region in 0..4 {
                        let clip = Rect::from_min_size(
                            Pos2::new((region % 2) as f32 * 425.0, (region / 2) as f32 * 265.0),
                            Vec2::new(425.0, 265.0),
                        );
                        let mut scene = Scene::default();
                        for i in 0..1000 {
                            let p = clip.min
                                + Vec2::new(
                                    10.0 + (i * 47 % 400) as f32,
                                    10.0 + (i * 71 % 240) as f32,
                                );
                            scene.disc(
                                p,
                                0.8 + (self.frame % 7) as f32 * 0.1,
                                Color32::BLUE,
                                Some(Emission::new(Color32::BLUE, 0.5)),
                            );
                        }
                        scene.path(
                            (0..513).map(|i| {
                                clip.min
                                    + Vec2::new(
                                        i as f32 / 512.0 * 425.0,
                                        132.0
                                            + (i as f32 * 0.03 + self.frame as f32 * 0.1).sin()
                                                * 90.0,
                                    )
                            }),
                            Stroke::new(1.5, Color32::BLUE),
                            Some(Emission::new(Color32::BLUE, 1.0)),
                        );
                        frame
                            .submit(egui::Id::unique(region), self.frame + 1, clip, clip, scene)
                            .unwrap();
                    }
                } else {
                    let mut scene = Scene::default();
                    for (x, color) in [
                        (0.2, Color32::BLUE),
                        (0.5, Color32::RED),
                        (0.8, Color32::from_rgb(255, 0, 255)),
                    ] {
                        scene.disc(
                            bounds.min + Vec2::new(bounds.width() * x, 130.0),
                            3.0,
                            Color32::WHITE,
                            Some(Emission::new(color, 2.0)),
                        );
                    }
                    // Red and blue truly overlap at the final point.
                    for color in [Color32::RED, Color32::BLUE] {
                        scene.disc(
                            Pos2::new(bounds.width() * 0.5, 230.0),
                            3.0,
                            Color32::TRANSPARENT,
                            Some(Emission::new(color, 2.0)),
                        );
                    }
                    scene.dotted_axis(
                        Pos2::new(20.0, 310.0),
                        Pos2::new(830.0, 310.0),
                        12.0,
                        1.0,
                        Color32::BLUE,
                        Some(Emission::new(Color32::BLUE, 1.0)),
                    );
                    scene.path(
                        [Pos2::new(40.0, 400.0), Pos2::new(810.0, 400.0)],
                        Stroke::new(1.0, Color32::WHITE),
                        None,
                    );
                    frame
                        .submit(
                            egui::Id::unique("fixture"),
                            1,
                            bounds,
                            Rect::from_min_max(bounds.min, Pos2::new(bounds.right(), 330.0)),
                            scene,
                        )
                        .unwrap();
                    let mut edge = Scene::default();
                    edge.disc(
                        Pos2::new(750.0, 380.0),
                        2.0,
                        Color32::GREEN,
                        Some(Emission::new(Color32::GREEN, 1.0)),
                    );
                    let clip = Rect::from_min_max(Pos2::new(750.0, 350.0), Pos2::new(830.0, 410.0));
                    frame
                        .submit(egui::Id::unique("edge"), 1, clip, clip.expand(12.0), edge)
                        .unwrap();
                    if self.mode == "empty" && self.frame >= 60 {
                        frame.regions.clear();
                        frame
                            .submit(
                                egui::Id::unique("empty"),
                                2,
                                bounds,
                                bounds,
                                Scene::default(),
                            )
                            .unwrap();
                    }
                    if self.mode == "removed" && self.frame >= 60 {
                        frame.regions.clear();
                        let mut scene = Scene::default();
                        scene.disc(
                            Pos2::new(425.0, 130.0),
                            3.0,
                            Color32::RED,
                            Some(Emission::new(Color32::RED, 2.0)),
                        );
                        frame
                            .submit(egui::Id::unique("remaining"), 2, bounds, bounds, scene)
                            .unwrap();
                    }
                }
                let health = self.renderer.finish(frame);
                if measured {
                    let timing = self.gpu.clone();
                    ui.painter().add(egui::Shape::Callback(egui::PaintCallback {
                        rect: bounds,
                        callback: Arc::new(eframe::egui_glow::CallbackFn::new(
                            move |_, p| unsafe {
                                let gl = p.gl();
                                gl.end_query(glow::TIME_ELAPSED);
                                let mut t = timing.lock().unwrap();
                                let q = t.query.take().unwrap();
                                t.samples.push(
                                    gl.get_query_parameter_u64(q, glow::QUERY_RESULT) as f64
                                        / 1_000_000.0,
                                );
                                gl.delete_query(q);
                            },
                        )),
                    }));
                }
                self.build_cpu
                    .push(started.elapsed().as_secs_f64() * 1000.0);
                ui.painter().text(
                    Pos2::new(40.0, 440.0),
                    egui::Align2::LEFT_TOP,
                    "Sharp foreground · RGBA16F",
                    egui::FontId::proportional(18.0),
                    Color32::WHITE,
                );
                if (150..166).contains(&self.frame) {
                    let path = self.directory.clone();
                    let done = self.done.clone();
                    let renderer = self.renderer.clone();
                    let expected_failure = self.failure;
                    let frame_cpu = self.frame_cpu.clone();
                    let build_cpu = self.build_cpu.clone();
                    let gpu = self.gpu.clone();
                    let captures=self.captures.clone();let requested=std::time::Instant::now();let last=self.frame==165;
                    ctx.request_screenshot(move |image| {
                        captures.lock().unwrap().push(requested.elapsed().as_secs_f64()*1000.0);
                        let error = health.lock().unwrap().clone();
                        assert_eq!(
                            error.is_some(),
                            expected_failure,
                            "Unexpected renderer health: {error:?}"
                        );
                        std::fs::create_dir_all(&path).unwrap();
                        std::fs::write(
                            path.join("resources.json"),
                            serde_json::to_vec_pretty(&renderer.resources()).unwrap(),
                        )
                        .unwrap();
                        std::fs::write(
                            path.join("gpu-total.json"),
                            serde_json::to_vec_pretty(&gpu.lock().unwrap().samples).unwrap(),
                        )
                        .unwrap();
                        std::fs::write(
                            path.join("cpu.json"),
                            serde_json::to_vec_pretty(
                                &serde_json::json!({"frame_ms":frame_cpu,"build_ms":build_cpu}),
                            )
                            .unwrap(),
                        )
                        .unwrap();
                        if let Some(error) = error {
                            std::fs::write(path.join("error.txt"), error).unwrap();
                            done.store(true, Ordering::Release);
                            return;
                        }
                        if !last { return; }
                        let encode_start=std::time::Instant::now();
                        let file = std::fs::File::create(path.join("fixture.png")).unwrap();
                        let mut encoder =
                            png::Encoder::new(file, image.size[0] as u32, image.size[1] as u32);
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        encoder
                            .write_header()
                            .unwrap()
                            .write_image_data(
                                &image
                                    .pixels
                                    .iter()
                                    .flat_map(|p| p.to_array())
                                    .collect::<Vec<_>>(),
                            )
                            .unwrap();
                        std::fs::write(path.join("capture-timing.json"),serde_json::to_vec_pretty(&serde_json::json!({"request_to_pixels_ms":*captures.lock().unwrap(),"final_png_encode_write_ms":encode_start.elapsed().as_secs_f64()*1000.0,"physical_dimensions":image.size})).unwrap()).unwrap();
                        std::fs::write(
                            path.join("diagnostics.json"),
                            serde_json::to_vec_pretty(&renderer.diagnostics()).unwrap(),
                        )
                        .unwrap();
                        done.store(true, Ordering::Release);
                    });
                }
            });
        self.frame += 1;
        ctx.request_repaint();
    }
}
fn main() -> eframe::Result {
    let scale: f32 = std::env::var("WAVEBOB_VISUALIZATION_REVIEW_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let directory = std::env::var_os("WAVEBOB_VISUALIZATION_REVIEW_DIR")
        .map(PathBuf::from)
        .expect("Set WAVEBOB_VISUALIZATION_REVIEW_DIR");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([850.0 * scale, 530.0 * scale]),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Visualization review",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_pixels_per_point(scale);
            let renderer = Renderer::review();
            let mode = std::env::var("WAVEBOB_VISUALIZATION_REVIEW_CASE").unwrap_or_default();
            let renderer = if mode.starts_with("reverb")
                || mode.starts_with("distortion")
                || mode == "dense"
            {
                Renderer::review_timings()
            } else {
                renderer
            };
            let failure = std::env::var_os("WAVEBOB_VISUALIZATION_REVIEW_FAIL").is_some();
            if failure {
                if mode == "budget-error" {
                    renderer.review_budget_failure();
                } else {
                    renderer.review_failure();
                }
            }
            Ok(Box::new(Review {
                renderer,
                frame: 0,
                done: Arc::default(),
                directory,
                failure,
                mode,
                frame_cpu: Vec::new(),
                build_cpu: Vec::new(),
                curve: wavebob::dsp::fast_distortion::Curve::new(
                    wavebob::dsp::fast_distortion::DEFAULTS,
                )
                .unwrap(),
                gpu: Arc::default(),
                captures: Arc::default(),
            }))
        }),
    )
}
fn reverb_instance() -> wavebob::processing::Instance {
    use wavebob::{
        dsp::reverb as r,
        processing::{EffectKind, Instance, ParameterValue},
    };
    Instance {
        id: "review".into(),
        kind: EffectKind::Reverb,
        bypassed: false,
        parameters: r::IDS
            .into_iter()
            .zip(r::DEFAULTS)
            .map(|(id, value)| (id.into(), ParameterValue::Integer(value)))
            .collect(),
    }
}
