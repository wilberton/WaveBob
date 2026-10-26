//! The sole bloom implementation. eframe 0.37 switches one GL context between
//! native surfaces; context identity is nevertheless checked before every use.
use super::*;
use eframe::{
    egui_glow,
    glow::{self, HasContext},
};
use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
    time::Instant,
};

const MAX_SIDE: i32 = 4096;
const BUDGET: usize = 128 * 1024 * 1024;
// Entire pooled emission participates at every level. Maximum support keeps
// existing caller padding intact; normalized weights preserve emitted energy.
const PROFILE: [(f32, f32); 3] = [(3.0, 0.15), (6.0, 0.25), (12.0, 0.60)];
const TARGET_COUNT: u64 = PROFILE.len() as u64 + 3;
pub type Health = Arc<Mutex<Option<String>>>;

#[derive(Clone, Default)]
pub struct Renderer(Arc<Mutex<State>>);
#[derive(Default)]
struct State {
    windows: HashMap<egui::ViewportId, Window>,
    retired: Vec<egui::ViewportId>,
    serial: u64,
    probe: bool,
    failures: HashMap<egui::ViewportId, ([u32; 2], usize, String)>,
    force_error: bool,
    energy: bool,
    timers: bool,
    allocations: u64,
    releases: u64,
    review_budget: Option<usize>,
}
#[derive(Clone, Default, serde::Serialize)]
pub struct Diagnostics {
    pub driver: String,
    pub bytes: usize,
    pub allocations: u64,
    pub rebuilds: u64,
    pub passes: u64,
    pub cached: u64,
    pub cpu_ms: Vec<f64>,
    pub gpu_ms: Vec<f64>,
    pub source_energy: [f64; 3],
    pub bloom_energy: [f64; 3],
    pub levels: Vec<LevelDiagnostics>,
    pub error: Option<String>,
}
#[derive(Clone, Default, serde::Serialize)]
pub struct LevelDiagnostics {
    pub radius: f32,
    pub weight: f32,
    pub energy: [f64; 3],
}
struct Window {
    gl: Arc<glow::Context>,
    programs: Programs,
    targets: Targets,
    key: u64,
    seen: u64,
    diagnostics: Diagnostics,
}
struct Programs {
    geometry: glow::Program,
    blur: glow::Program,
    composite: glow::Program,
    vao: glow::VertexArray,
    buffer: glow::Buffer,
}
struct Target {
    texture: glow::Texture,
    fbo: glow::Framebuffer,
}
struct Targets {
    size: [i32; 2],
    crop: Rect,
    half: [i32; 2],
    emission: Target,
    scratch: Target,
    bloom: [Target; 3],
    background: Target,
    bytes: usize,
}

impl Renderer {
    /// Verification seam. Enables readback/timer instrumentation only in review.
    pub fn review() -> Self {
        let r = Self::default();
        {
            let mut s = r.0.lock().unwrap();
            s.probe = true;
            s.energy = true;
            s.timers = true;
        }
        r
    }
    pub fn review_timings() -> Self {
        let r = Self::review();
        {
            let mut s = r.0.lock().unwrap();
            s.energy = false;
            s.timers = false;
        }
        r
    }
    pub fn resources(&self) -> serde_json::Value {
        let s = self.0.lock().unwrap();
        serde_json::json!({"live_viewports":s.windows.len(),"live_texture_bytes":s.windows.values().map(|w|w.targets.bytes).sum::<usize>(),"target_allocations":s.allocations,"target_releases":s.releases,"failures":s.failures.len(),"texture_budget":BUDGET,"viewport_budget":16})
    }
    pub fn review_failure(&self) {
        self.0.lock().unwrap().force_error = true;
    }
    pub fn review_budget_failure(&self) {
        self.0.lock().unwrap().review_budget = Some(1);
    }
    pub fn maintain(&self, painter: &egui::Painter) {
        if self.0.lock().unwrap().retired.is_empty() {
            return;
        }
        let state = self.0.clone();
        painter.add(Shape::Callback(egui::PaintCallback {
            rect: painter.clip_rect(),
            callback: Arc::new(egui_glow::CallbackFn::new(move |_, p| {
                let mut s = state.lock().unwrap();
                let retired = std::mem::take(&mut s.retired);
                for id in retired {
                    s.failures.remove(&id);
                    if let Some(w) = s.windows.remove(&id) {
                        s.releases += TARGET_COUNT;
                        unsafe {
                            w.destroy(p.gl());
                        }
                    }
                }
            })),
        }));
    }
    pub fn finish(&self, mut frame: Frame) -> Health {
        let health = Arc::new(Mutex::new(Some("Visualization did not complete".into())));
        let Some(slots) = frame.slot.take() else {
            *health.lock().unwrap() = None;
            return health;
        };
        let painter = &slots.painter;
        for (i, region) in frame.regions.iter().enumerate() {
            let clipped = painter.with_clip_rect(region.clip);
            let background = region.scene.background.clone();
            clipped.set(slots.background[i], Shape::Vec(background));
            clipped.set(
                slots.foreground[i],
                Shape::Vec(region.scene.marks.iter().map(|m| m.shape.clone()).collect()),
            );
        }
        let state = self.0.clone();
        let status = health.clone();
        let context = painter.ctx().clone();
        let bounds = frame.bounds;
        painter.set(
            slots.bloom,
            Shape::Callback(egui::PaintCallback {
                rect: bounds,
                callback: Arc::new(egui_glow::CallbackFn::new(move |info, painter| {
                    let result = state
                        .lock()
                        .unwrap()
                        .paint(&frame, &context, info, painter.gl());
                    *status.lock().unwrap() = result.err();
                })),
            }),
        );
        health
    }
    pub fn diagnostics(&self) -> Vec<(String, Diagnostics)> {
        self.0
            .lock()
            .unwrap()
            .windows
            .iter()
            .map(|(id, w)| (format!("{id:?}"), w.diagnostics.clone()))
            .collect()
    }
    pub fn retire(&self, id: egui::ViewportId) {
        let mut s = self.0.lock().unwrap();
        s.failures.remove(&id);
        s.retired.push(id);
    }
    pub fn destroy(&self, gl: &glow::Context) {
        let mut state = self.0.lock().unwrap();
        state.releases += state.windows.len() as u64 * TARGET_COUNT;
        state.failures.clear();
        state.retired.clear();
        for (_, w) in state.windows.drain() {
            unsafe {
                w.destroy(gl);
            }
        }
    }
}

impl State {
    fn paint(
        &mut self,
        frame: &Frame,
        _ctx: &egui::Context,
        info: egui::PaintCallbackInfo,
        gl: &Arc<glow::Context>,
    ) -> Result<(), String> {
        // The guard restores even shader/allocation failures. No readback is used.
        let guard = unsafe { SavedState::new(gl) };
        let context = Arc::as_ptr(gl) as usize;
        let result = if std::mem::take(&mut self.force_error) {
            unsafe {
                program(gl, GEOMETRY_VERTEX, "deliberately invalid shader")
                    .map(|p| gl.delete_program(p))
            }
        } else if let Some((size, owner, error)) = self.failures.get(&frame.viewport)
            && *size == info.screen_size_px
            && *owner == context
        {
            Err(error.clone())
        } else {
            self.failures.remove(&frame.viewport);
            unsafe { self.paint_inner(frame, &info, gl, guard.framebuffer) }
        };
        if let Err(error) = &result {
            self.failures.insert(
                frame.viewport,
                (info.screen_size_px, context, error.clone()),
            );
        }
        let expected = self.probe.then(|| guard.clone());
        unsafe {
            guard.restore(gl);
        }
        if let Some(expected) = expected
            && unsafe { SavedState::new(gl) } != expected
        {
            return Err("Visualization failed to restore OpenGL state".into());
        }
        result
    }
    unsafe fn paint_inner(
        &mut self,
        frame: &Frame,
        info: &egui::PaintCallbackInfo,
        gl: &Arc<glow::Context>,
        destination: Option<glow::Framebuffer>,
    ) -> Result<(), String> {
        unsafe {
            for id in self.retired.drain(..) {
                self.failures.remove(&id);
                if let Some(w) = self.windows.remove(&id) {
                    self.releases += TARGET_COUNT;
                    w.destroy(gl);
                }
            }
            self.serial += 1;
            let size = [info.screen_size_px[0] as i32, info.screen_size_px[1] as i32];
            let paint_clip = info
                .clip_rect
                .intersect(frame.bounds)
                .intersect(Rect::from_min_size(
                    Pos2::ZERO,
                    Vec2::new(size[0] as f32, size[1] as f32) / info.pixels_per_point,
                ));
            let crop = emission_bounds(
                frame,
                info.pixels_per_point,
                info.screen_size_px,
                paint_clip,
            );
            if size.iter().any(|s| *s <= 0 || *s > MAX_SIDE) {
                return Err("Visualization viewport exceeds 4096-pixel limit".into());
            }
            if self.windows.get(&frame.viewport).is_some_and(|w| {
                !Arc::ptr_eq(&w.gl, gl) || w.targets.size != size || w.targets.crop != crop
            }) {
                let old = self.windows.remove(&frame.viewport).unwrap();
                if Arc::ptr_eq(&old.gl, gl) {
                    self.releases += TARGET_COUNT;
                    old.destroy(gl);
                }
            }
            if !self.windows.contains_key(&frame.viewport) {
                let needed = Targets::bytes(size, crop);
                while self.windows.len() >= 16
                    || self
                        .windows
                        .values()
                        .map(|w| w.targets.bytes)
                        .sum::<usize>()
                        + needed
                        > self.review_budget.unwrap_or(BUDGET)
                {
                    let Some(id) = self
                        .windows
                        .iter()
                        .min_by_key(|(_, w)| w.seen)
                        .map(|(id, _)| *id)
                    else {
                        return Err("Visualization texture budget exceeded".into());
                    };
                    self.windows.remove(&id).unwrap().destroy(gl);
                    self.releases += TARGET_COUNT;
                }
                let programs = Programs::new(gl)?;
                let targets = match Targets::new(gl, size, crop) {
                    Ok(t) => t,
                    Err(e) => {
                        programs.destroy(gl);
                        return Err(e);
                    }
                };
                let driver = format!(
                    "{} / {} / {}",
                    gl.get_parameter_string(glow::VENDOR),
                    gl.get_parameter_string(glow::RENDERER),
                    gl.get_parameter_string(glow::VERSION)
                );
                self.allocations += TARGET_COUNT;
                self.windows.insert(
                    frame.viewport,
                    Window {
                        gl: gl.clone(),
                        programs,
                        targets,
                        key: 0,
                        seen: self.serial,
                        diagnostics: Diagnostics {
                            driver,
                            bytes: needed,
                            allocations: TARGET_COUNT,
                            ..Default::default()
                        },
                    },
                );
            }
            let w = self.windows.get_mut(&frame.viewport).unwrap();
            w.seen = self.serial;
            let started = Instant::now();
            let key = cache_key(frame, paint_clip);
            let rebuilt = w.key != key;
            let query = if self.timers {
                gl.create_query().ok()
            } else {
                None
            };
            if let Some(q) = query {
                gl.begin_query(glow::TIME_ELAPSED, q);
            }
            gl.disable(glow::DEPTH_TEST);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::FRAMEBUFFER_SRGB);
            gl.color_mask(true, true, true, true);
            gl.bind_vertex_array(Some(w.programs.vao));
            if w.key != key {
                gl.bind_framebuffer(glow::FRAMEBUFFER, Some(w.targets.emission.fbo));
                gl.viewport(0, 0, w.targets.half[0], w.targets.half[1]);
                gl.disable(glow::SCISSOR_TEST);
                gl.clear_color(0.0, 0.0, 0.0, 0.0);
                gl.clear(glow::COLOR_BUFFER_BIT);
                gl.enable(glow::BLEND);
                gl.blend_equation(glow::FUNC_ADD);
                gl.blend_func(glow::ONE, glow::ONE);
                gl.use_program(Some(w.programs.geometry));
                gl.uniform_2_f32(
                    gl.get_uniform_location(w.programs.geometry, "screen")
                        .as_ref(),
                    w.targets.half[0] as f32 * 2.0 / info.pixels_per_point,
                    w.targets.half[1] as f32 * 2.0 / info.pixels_per_point,
                );
                gl.uniform_2_f32(
                    gl.get_uniform_location(w.programs.geometry, "origin")
                        .as_ref(),
                    crop.left() / info.pixels_per_point,
                    crop.top() / info.pixels_per_point,
                );
                gl.bind_buffer(glow::ARRAY_BUFFER, Some(w.programs.buffer));
                gl.enable_vertex_attrib_array(0);
                gl.vertex_attrib_pointer_f32(0, 2, glow::FLOAT, false, 36, 0);
                gl.enable_vertex_attrib_array(1);
                gl.vertex_attrib_pointer_f32(1, 4, glow::FLOAT, false, 36, 8);
                gl.enable_vertex_attrib_array(2);
                gl.vertex_attrib_pointer_f32(2, 3, glow::FLOAT, false, 36, 24);
                for region in &frame.regions {
                    let source_clip = region.clip.intersect(paint_clip);
                    if !source_clip.is_positive() {
                        continue;
                    }
                    gl.uniform_4_f32(
                        gl.get_uniform_location(w.programs.geometry, "emission_clip")
                            .as_ref(),
                        source_clip.left(),
                        source_clip.top(),
                        source_clip.right(),
                        source_clip.bottom(),
                    );
                    scissor(
                        gl,
                        source_clip.translate(-crop.min.to_vec2() / info.pixels_per_point),
                        info.pixels_per_point * 0.5,
                        w.targets.half[1],
                    );
                    let mut vertices = Vec::<f32>::new();
                    // Emission never samples egui's pre-rasterized circle atlas.
                    let mut tessellator = egui::epaint::Tessellator::new(
                        info.pixels_per_point * 0.5,
                        egui::epaint::TessellationOptions {
                            prerasterized_discs: false,
                            round_line_segments_to_pixels: false,
                            ..Default::default()
                        },
                        [1, 1],
                        Vec::new(),
                    );
                    let mut batches: HashMap<u32, Vec<egui::epaint::ClippedShape>> = HashMap::new();
                    for mark in &region.scene.marks {
                        let Some(e) = mark.emission else {
                            continue;
                        };
                        if let Shape::Circle(circle) = &mark.shape {
                            let rgba = egui::Rgba::from(e.color);
                            let padding = 2.0 / info.pixels_per_point;
                            let r = circle.radius + padding;
                            for offset in [
                                Vec2::new(-r, -r),
                                Vec2::new(r, -r),
                                Vec2::new(r, r),
                                Vec2::new(-r, -r),
                                Vec2::new(r, r),
                                Vec2::new(-r, r),
                            ] {
                                let p = circle.center + offset;
                                vertices.extend_from_slice(&[
                                    p.x,
                                    p.y,
                                    rgba[0] * e.intensity,
                                    rgba[1] * e.intensity,
                                    rgba[2] * e.intensity,
                                    rgba[3],
                                    circle.center.x,
                                    circle.center.y,
                                    circle.radius,
                                ]);
                            }
                            continue;
                        }
                        let mut shape = mark.shape.clone();
                        recolor(&mut shape, e.color);
                        batches.entry(e.intensity.to_bits()).or_default().push(
                            egui::epaint::ClippedShape {
                                clip_rect: source_clip,
                                shape,
                                transform_after_tessellation: egui::emath::TSTransform::IDENTITY,
                            },
                        );
                    }
                    for (intensity, shapes) in batches {
                        let intensity = f32::from_bits(intensity);
                        for primitive in tessellator.tessellate_shapes(shapes) {
                            if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                                for index in mesh.indices {
                                    let v = mesh.vertices[index as usize];
                                    let rgba = egui::Rgba::from(v.color);
                                    vertices.extend_from_slice(&[
                                        v.pos.x,
                                        v.pos.y,
                                        rgba[0] * intensity,
                                        rgba[1] * intensity,
                                        rgba[2] * intensity,
                                        rgba[3],
                                        0.0,
                                        0.0,
                                        -1.0,
                                    ]);
                                }
                            }
                        }
                    }
                    let bytes = std::slice::from_raw_parts(
                        vertices.as_ptr().cast::<u8>(),
                        vertices.len() * 4,
                    );
                    gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STREAM_DRAW);
                    gl.draw_arrays(glow::TRIANGLES, 0, (vertices.len() / 9) as i32);
                }
                gl.disable_vertex_attrib_array(0);
                gl.disable_vertex_attrib_array(1);
                gl.disable_vertex_attrib_array(2);
                gl.disable(glow::BLEND);
                gl.disable(glow::SCISSOR_TEST);
                // Three normalized complete-image Gaussian blurs; one scratch
                // is reused sequentially, never partitioned by emitter.
                gl.use_program(Some(w.programs.blur));
                gl.active_texture(glow::TEXTURE0);
                gl.uniform_1_i32(
                    gl.get_uniform_location(w.programs.blur, "source").as_ref(),
                    0,
                );
                for ((radius, _), bloom) in PROFILE.iter().zip(&w.targets.bloom) {
                    let step = info.pixels_per_point * radius / 16.0;
                    for (source, target, x, y) in [
                        (
                            w.targets.emission.texture,
                            w.targets.scratch.fbo,
                            step / w.targets.half[0] as f32,
                            0.0,
                        ),
                        (
                            w.targets.scratch.texture,
                            bloom.fbo,
                            0.0,
                            step / w.targets.half[1] as f32,
                        ),
                    ] {
                        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target));
                        gl.bind_texture(glow::TEXTURE_2D, Some(source));
                        gl.uniform_2_f32(
                            gl.get_uniform_location(w.programs.blur, "direction")
                                .as_ref(),
                            x,
                            y,
                        );
                        gl.draw_arrays(glow::TRIANGLES, 0, 3);
                    }
                }
                w.key = key;
                w.diagnostics.rebuilds += 1;
                w.diagnostics.passes += 1 + 2 * PROFILE.len() as u64;
            } else {
                w.diagnostics.cached += 1;
            }
            // Snapshot existing gamma-space UI, then add light in linear space.
            gl.bind_framebuffer(glow::FRAMEBUFFER, destination);
            gl.active_texture(glow::TEXTURE1);
            gl.bind_texture(glow::TEXTURE_2D, Some(w.targets.background.texture));
            gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, size[0], size[1]);
            gl.viewport(0, 0, size[0], size[1]);
            gl.disable(glow::BLEND);
            gl.use_program(Some(w.programs.composite));
            for (i, unit) in [0, 2, 3].into_iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + unit);
                gl.bind_texture(glow::TEXTURE_2D, Some(w.targets.bloom[i].texture));
                gl.uniform_1_i32(
                    gl.get_uniform_location(w.programs.composite, &format!("level{i}"))
                        .as_ref(),
                    unit as i32,
                );
            }
            gl.uniform_3_f32(
                gl.get_uniform_location(w.programs.composite, "weights")
                    .as_ref(),
                PROFILE[0].1,
                PROFILE[1].1,
                PROFILE[2].1,
            );
            gl.uniform_1_i32(
                gl.get_uniform_location(w.programs.composite, "background")
                    .as_ref(),
                1,
            );
            gl.uniform_4_f32(
                gl.get_uniform_location(w.programs.composite, "crop")
                    .as_ref(),
                crop.left(),
                size[1] as f32 - crop.bottom(),
                crop.width(),
                crop.height(),
            );
            gl.uniform_2_f32(
                gl.get_uniform_location(w.programs.composite, "screen")
                    .as_ref(),
                size[0] as f32,
                size[1] as f32,
            );
            for region in &frame.regions {
                let clip = region.glow_clip.intersect(paint_clip);
                if !clip.is_positive() {
                    continue;
                }
                let ppp = info.pixels_per_point;
                gl.uniform_4_f32(
                    gl.get_uniform_location(w.programs.composite, "glow_clip")
                        .as_ref(),
                    clip.left() * ppp,
                    size[1] as f32 - clip.bottom() * ppp,
                    clip.right() * ppp,
                    size[1] as f32 - clip.top() * ppp,
                );
                scissor(gl, clip, ppp, size[1]);
                gl.draw_arrays(glow::TRIANGLES, 0, 3);
            }
            let cpu_ms = started.elapsed().as_secs_f64() * 1000.0;
            if let Some(q) = query {
                gl.end_query(glow::TIME_ELAPSED);
                if w.diagnostics.gpu_ms.len() >= 256 {
                    w.diagnostics.gpu_ms.remove(0);
                }
                w.diagnostics
                    .gpu_ms
                    .push(gl.get_query_parameter_u64(q, glow::QUERY_RESULT) as f64 / 1_000_000.0);
                gl.delete_query(q);
            }
            if self.energy && rebuilt {
                w.diagnostics.source_energy = energy(gl, &w.targets.emission, w.targets.half);
                w.diagnostics.levels = PROFILE
                    .iter()
                    .zip(&w.targets.bloom)
                    .map(|((radius, weight), target)| LevelDiagnostics {
                        radius: *radius,
                        weight: *weight,
                        energy: energy(gl, target, w.targets.half),
                    })
                    .collect();
                w.diagnostics.bloom_energy = std::array::from_fn(|c| {
                    w.diagnostics
                        .levels
                        .iter()
                        .map(|l| l.energy[c] * l.weight as f64)
                        .sum()
                });
            }
            let error = gl.get_error();
            if error != glow::NO_ERROR {
                let e = format!("Visualization OpenGL error: 0x{error:x}");
                w.diagnostics.error = Some(e.clone());
                return Err(e);
            }
            if w.diagnostics.cpu_ms.len() >= 256 {
                w.diagnostics.cpu_ms.remove(0);
            }
            w.diagnostics.cpu_ms.push(cpu_ms);
            Ok(())
        }
    }
}
fn cache_key(frame: &Frame, paint_clip: Rect) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    frame.viewport.hash(&mut h);
    for (radius, weight) in PROFILE {
        radius.to_bits().hash(&mut h);
        weight.to_bits().hash(&mut h);
    }
    for v in [
        paint_clip.min.x,
        paint_clip.min.y,
        paint_clip.max.x,
        paint_clip.max.y,
    ] {
        v.to_bits().hash(&mut h);
    }
    frame.pixels_per_point.to_bits().hash(&mut h);
    for r in &frame.regions {
        r.id.hash(&mut h);
        r.revision.hash(&mut h);
        for v in [r.clip.min, r.clip.max, r.glow_clip.min, r.glow_clip.max] {
            v.x.to_bits().hash(&mut h);
            v.y.to_bits().hash(&mut h);
        }
    }
    h.finish()
}
fn emission_bounds(frame: &Frame, scale: f32, size: [u32; 2], paint_clip: Rect) -> Rect {
    let mut bounds = Rect::NOTHING;
    for region in &frame.regions {
        if region
            .scene
            .marks
            .iter()
            .any(|m| m.emission.is_some_and(|e| e.intensity > 0.0))
        {
            let source = region.clip.intersect(paint_clip);
            if source.is_positive() {
                // Source clipping does not remove the zero padding needed by
                // the complete kernel. The final mask protects surrounding UI.
                bounds = bounds.union(
                    source
                        .expand(PROFILE[2].0)
                        .union(region.glow_clip.intersect(paint_clip)),
                );
            }
        }
    }
    bounds = bounds.intersect(Rect::from_min_size(
        Pos2::ZERO,
        Vec2::new(size[0] as f32, size[1] as f32) / scale,
    ));
    if !bounds.is_positive() {
        return Rect::from_min_size(Pos2::ZERO, Vec2::splat(2.0));
    }
    Rect::from_min_max(
        (bounds.min.to_vec2() * scale / 2.0).floor().to_pos2() * 2.0,
        (bounds.max.to_vec2() * scale / 2.0).ceil().to_pos2() * 2.0,
    )
}
unsafe fn energy(gl: &glow::Context, target: &Target, size: [i32; 2]) -> [f64; 3] {
    unsafe {
        let mut bytes = vec![0u8; size[0] as usize * size[1] as usize * 16];
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(target.fbo));
        gl.read_pixels(
            0,
            0,
            size[0],
            size[1],
            glow::RGBA,
            glow::FLOAT,
            glow::PixelPackData::Slice(Some(&mut bytes)),
        );
        let mut sum = [0.0; 3];
        for pixel in bytes.as_chunks::<16>().0 {
            for c in 0..3 {
                sum[c] += f32::from_ne_bytes(pixel[c * 4..c * 4 + 4].try_into().unwrap()) as f64;
            }
        }
        sum
    }
}
fn recolor(shape: &mut Shape, color: Color32) {
    match shape {
        Shape::Circle(s) => s.fill = color,
        Shape::Path(s) => {
            if s.closed {
                s.fill = color;
            }
            s.stroke.color = egui::epaint::ColorMode::Solid(color);
        }
        Shape::LineSegment { stroke, .. } => stroke.color = color,
        _ => {}
    }
}
unsafe fn scissor(gl: &glow::Context, rect: Rect, scale: f32, height: i32) {
    unsafe {
        gl.enable(glow::SCISSOR_TEST);
        let min = (rect.min.to_vec2() * scale).floor();
        let max = (rect.max.to_vec2() * scale).ceil();
        gl.scissor(
            min.x as i32,
            height - max.y as i32,
            (max.x - min.x).max(0.0) as i32,
            (max.y - min.y).max(0.0) as i32,
        );
    }
}
impl Window {
    unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            self.targets.destroy(gl);
            self.programs.destroy(gl);
        }
    }
}
impl Targets {
    fn bytes(size: [i32; 2], crop: Rect) -> usize {
        (size[0] * size[1] * 4) as usize
            + (crop.width() * crop.height() * ((PROFILE.len() + 2) * 2) as f32) as usize
    }
    unsafe fn new(gl: &glow::Context, size: [i32; 2], crop: Rect) -> Result<Self, String> {
        unsafe {
            let half = [(crop.width() / 2.0) as i32, (crop.height() / 2.0) as i32];
            if size
                .iter()
                .any(|s| *s > gl.get_parameter_i32(glow::MAX_TEXTURE_SIZE))
            {
                return Err("Visualization texture size unsupported".into());
            }
            let mut targets = Vec::new();
            for (dimensions, format) in [
                (half, glow::RGBA16F),
                (half, glow::RGBA16F),
                (half, glow::RGBA16F),
                (half, glow::RGBA16F),
                (half, glow::RGBA16F),
                (size, glow::RGBA8),
            ] {
                match Target::new(gl, dimensions, format) {
                    Ok(t) => targets.push(t),
                    Err(e) => {
                        for t in targets {
                            t.destroy(gl);
                        }
                        return Err(e);
                    }
                }
            }
            let background = targets.pop().unwrap();
            let bloom = targets.split_off(2).try_into().ok().unwrap();
            let scratch = targets.pop().unwrap();
            let emission = targets.pop().unwrap();
            Ok(Self {
                size,
                crop,
                half,
                emission,
                scratch,
                bloom,
                background,
                bytes: Self::bytes(size, crop),
            })
        }
    }
    unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            for t in [self.emission, self.scratch, self.background]
                .into_iter()
                .chain(self.bloom)
            {
                t.destroy(gl);
            }
        }
    }
}
impl Target {
    unsafe fn new(gl: &glow::Context, size: [i32; 2], format: u32) -> Result<Self, String> {
        unsafe {
            let texture = gl.create_texture()?;
            gl.bind_texture(glow::TEXTURE_2D, Some(texture));
            for (name, value) in [
                (glow::TEXTURE_MIN_FILTER, glow::LINEAR),
                (glow::TEXTURE_MAG_FILTER, glow::LINEAR),
                (glow::TEXTURE_WRAP_S, glow::CLAMP_TO_EDGE),
                (glow::TEXTURE_WRAP_T, glow::CLAMP_TO_EDGE),
            ] {
                gl.tex_parameter_i32(glow::TEXTURE_2D, name, value as i32);
            }
            gl.tex_image_2d(
                glow::TEXTURE_2D,
                0,
                format as i32,
                size[0],
                size[1],
                0,
                glow::RGBA,
                glow::FLOAT,
                glow::PixelUnpackData::Slice(None),
            );
            let fbo = match gl.create_framebuffer() {
                Ok(f) => f,
                Err(e) => {
                    gl.delete_texture(texture);
                    return Err(e);
                }
            };
            gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
            gl.framebuffer_texture_2d(
                glow::FRAMEBUFFER,
                glow::COLOR_ATTACHMENT0,
                glow::TEXTURE_2D,
                Some(texture),
                0,
            );
            let result = Self { texture, fbo };
            if gl.check_framebuffer_status(glow::FRAMEBUFFER) != glow::FRAMEBUFFER_COMPLETE {
                result.destroy(gl);
                return Err("Visualization RGBA16F framebuffer unsupported".into());
            }
            Ok(result)
        }
    }
    unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            gl.delete_framebuffer(self.fbo);
            gl.delete_texture(self.texture);
        }
    }
}
impl Programs {
    unsafe fn new(gl: &glow::Context) -> Result<Self, String> {
        unsafe {
            let geometry = program(gl, GEOMETRY_VERTEX, GEOMETRY_FRAGMENT)?;
            let blur = match program(gl, FULLSCREEN_VERTEX, BLUR_FRAGMENT) {
                Ok(p) => p,
                Err(e) => {
                    gl.delete_program(geometry);
                    return Err(e);
                }
            };
            let composite = match program(gl, FULLSCREEN_VERTEX, COMPOSITE_FRAGMENT) {
                Ok(p) => p,
                Err(e) => {
                    gl.delete_program(geometry);
                    gl.delete_program(blur);
                    return Err(e);
                }
            };
            let vao = match gl.create_vertex_array() {
                Ok(v) => v,
                Err(e) => {
                    for p in [geometry, blur, composite] {
                        gl.delete_program(p);
                    }
                    return Err(e);
                }
            };
            let buffer = match gl.create_buffer() {
                Ok(b) => b,
                Err(e) => {
                    gl.delete_vertex_array(vao);
                    for p in [geometry, blur, composite] {
                        gl.delete_program(p);
                    }
                    return Err(e);
                }
            };
            Ok(Self {
                geometry,
                blur,
                composite,
                vao,
                buffer,
            })
        }
    }
    unsafe fn destroy(self, gl: &glow::Context) {
        unsafe {
            for p in [self.geometry, self.blur, self.composite] {
                gl.delete_program(p);
            }
            gl.delete_vertex_array(self.vao);
            gl.delete_buffer(self.buffer);
        }
    }
}
unsafe fn program(
    gl: &glow::Context,
    vertex: &str,
    fragment: &str,
) -> Result<glow::Program, String> {
    unsafe {
        let p = gl.create_program()?;
        for (kind, source) in [
            (glow::VERTEX_SHADER, vertex),
            (glow::FRAGMENT_SHADER, fragment),
        ] {
            let s = match gl.create_shader(kind) {
                Ok(s) => s,
                Err(e) => {
                    gl.delete_program(p);
                    return Err(e);
                }
            };
            gl.shader_source(s, source);
            gl.compile_shader(s);
            if !gl.get_shader_compile_status(s) {
                let error = gl.get_shader_info_log(s);
                gl.delete_shader(s);
                gl.delete_program(p);
                return Err(format!("Visualization shader: {error}"));
            }
            gl.attach_shader(p, s);
            gl.delete_shader(s);
        }
        gl.link_program(p);
        if !gl.get_program_link_status(p) {
            let e = gl.get_program_info_log(p);
            gl.delete_program(p);
            return Err(format!("Visualization program: {e}"));
        }
        Ok(p)
    }
}
#[derive(Clone, Debug, PartialEq)]
struct SavedState {
    framebuffer: Option<glow::Framebuffer>,
    program: Option<glow::Program>,
    vao: Option<glow::VertexArray>,
    buffer: Option<glow::Buffer>,
    viewport: [i32; 4],
    scissor: [i32; 4],
    texture: [Option<glow::Texture>; 4],
    active: i32,
    blend: bool,
    scissored: bool,
    srgb: bool,
    depth: bool,
    cull: bool,
    mask: [bool; 4],
    blend_params: [i32; 6],
    clear: [f32; 4],
}
impl SavedState {
    unsafe fn new(gl: &glow::Context) -> Self {
        unsafe {
            let mut viewport = [0; 4];
            let mut scissor = [0; 4];
            let mut clear = [0.0; 4];
            gl.get_parameter_i32_slice(glow::VIEWPORT, &mut viewport);
            gl.get_parameter_i32_slice(glow::SCISSOR_BOX, &mut scissor);
            gl.get_parameter_f32_slice(glow::COLOR_CLEAR_VALUE, &mut clear);
            let active = gl.get_parameter_i32(glow::ACTIVE_TEXTURE);
            let mut mask = [0; 4];
            gl.get_parameter_i32_slice(glow::COLOR_WRITEMASK, &mut mask);
            let mut texture = [None; 4];
            for (i, t) in texture.iter_mut().enumerate() {
                gl.active_texture(glow::TEXTURE0 + i as u32);
                *t = gl.get_parameter_texture(glow::TEXTURE_BINDING_2D);
            }
            gl.active_texture(active as u32);
            Self {
                framebuffer: gl.get_parameter_framebuffer(glow::FRAMEBUFFER_BINDING),
                program: gl.get_parameter_program(glow::CURRENT_PROGRAM),
                vao: gl.get_parameter_vertex_array(glow::VERTEX_ARRAY_BINDING),
                buffer: gl.get_parameter_buffer(glow::ARRAY_BUFFER_BINDING),
                viewport,
                scissor,
                texture,
                active,
                blend: gl.is_enabled(glow::BLEND),
                scissored: gl.is_enabled(glow::SCISSOR_TEST),
                srgb: gl.is_enabled(glow::FRAMEBUFFER_SRGB),
                depth: gl.is_enabled(glow::DEPTH_TEST),
                cull: gl.is_enabled(glow::CULL_FACE),
                mask: mask.map(|v| v != 0),
                blend_params: [
                    glow::BLEND_SRC_RGB,
                    glow::BLEND_DST_RGB,
                    glow::BLEND_SRC_ALPHA,
                    glow::BLEND_DST_ALPHA,
                    glow::BLEND_EQUATION_RGB,
                    glow::BLEND_EQUATION_ALPHA,
                ]
                .map(|p| gl.get_parameter_i32(p)),
                clear,
            }
        }
    }
    unsafe fn restore(self, gl: &glow::Context) {
        unsafe {
            gl.bind_framebuffer(glow::FRAMEBUFFER, self.framebuffer);
            gl.use_program(self.program);
            gl.bind_vertex_array(self.vao);
            gl.bind_buffer(glow::ARRAY_BUFFER, self.buffer);
            gl.viewport(
                self.viewport[0],
                self.viewport[1],
                self.viewport[2],
                self.viewport[3],
            );
            gl.scissor(
                self.scissor[0],
                self.scissor[1],
                self.scissor[2],
                self.scissor[3],
            );
            for (i, t) in self.texture.into_iter().enumerate() {
                gl.active_texture(glow::TEXTURE0 + i as u32);
                gl.bind_texture(glow::TEXTURE_2D, t);
            }
            gl.active_texture(self.active as u32);
            for (cap, on) in [
                (glow::BLEND, self.blend),
                (glow::SCISSOR_TEST, self.scissored),
                (glow::FRAMEBUFFER_SRGB, self.srgb),
                (glow::DEPTH_TEST, self.depth),
                (glow::CULL_FACE, self.cull),
            ] {
                if on {
                    gl.enable(cap);
                } else {
                    gl.disable(cap);
                }
            }
            let b = self.blend_params;
            gl.blend_func_separate(b[0] as u32, b[1] as u32, b[2] as u32, b[3] as u32);
            gl.blend_equation_separate(b[4] as u32, b[5] as u32);
            gl.clear_color(self.clear[0], self.clear[1], self.clear[2], self.clear[3]);
            gl.color_mask(self.mask[0], self.mask[1], self.mask[2], self.mask[3]);
        }
    }
}
// Exact circle/pixel intersection keeps subpixel source energy stable with DPI.
const GEOMETRY_VERTEX: &str = r#"#version 330 core
layout(location=0) in vec2 position;
layout(location=1) in vec4 color;
layout(location=2) in vec3 disc;
uniform vec2 screen; uniform vec2 origin;
out vec4 light; out vec2 point; flat out vec3 circle;
void main(){vec2 p=position-origin;gl_Position=vec4(2.*p.x/screen.x-1.,1.-2.*p.y/screen.y,0,1);light=color;point=position;circle=disc;}
"#;
const GEOMETRY_FRAGMENT: &str = r#"#version 330 core
in vec4 light; in vec2 point; flat in vec3 circle; out vec4 result;
uniform vec4 emission_clip;
float quadrant(vec2 p,float r){
    float signum=sign(p.x)*sign(p.y); p=min(abs(p),vec2(r));
    if(dot(p,p)<=r*r)return signum*p.x*p.y;
    if(p.x>=r && p.y>=r)return signum*0.785398163397*r*r;
    vec2 root=sqrt(max(vec2(0),vec2(r*r)-p*p));
    return signum*0.5*(dot(p,root)+r*r*(asin(p.x/r)+asin(p.y/r)-1.570796326795));
}
void main(){
    vec2 pixel=abs(vec2(dFdx(point.x),dFdy(point.y)));
    vec2 lo=max(point-pixel*.5,emission_clip.xy),hi=min(point+pixel*.5,emission_clip.zw);
    float coverage=clamp((hi.x-lo.x)/pixel.x,0.,1.)*clamp((hi.y-lo.y)/pixel.y,0.,1.);
    if(circle.z>0.){
        lo=(lo-circle.xy)/pixel;hi=(hi-circle.xy)/pixel;float r=circle.z/pixel.x;
        vec2 nearest=max(max(lo,-hi),vec2(0));
        if(coverage<=0. || dot(nearest,nearest)>=r*r)discard;
        coverage=coverage>0.?clamp(quadrant(hi,r)-quadrant(vec2(lo.x,hi.y),r)-quadrant(vec2(hi.x,lo.y),r)+quadrant(lo,r),0.,1.):0.;
    }
    result=vec4(light.rgb*coverage,0);
}
"#;
const FULLSCREEN_VERTEX: &str = "#version 330 core\nout vec2 uv;void main(){vec2 p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);uv=p;gl_Position=vec4(p*2.-1.,0,1);}";
// Normalized whole-image Gaussian, paired bilinear fetches (no per-pixel exp).
const BLUR_FRAGMENT: &str = r#"#version 330 core
in vec2 uv;uniform sampler2D source;uniform vec2 direction;out vec4 result;
void main(){
 result=texture(source,uv)*0.159675942;
 result+=(texture(source,uv+direction*1.440286351)+texture(source,uv-direction*1.440286351))*0.263348004;
 result+=(texture(source,uv+direction*3.363547460)+texture(source,uv-direction*3.363547460))*0.122118493;
 result+=(texture(source,uv+direction*5.293177779)+texture(source,uv-direction*5.293177779))*0.030573160;
 result+=(texture(source,uv+direction*7.231475217)+texture(source,uv-direction*7.231475217))*0.004122373;
}
"#;
const COMPOSITE_FRAGMENT: &str = r#"#version 330 core
in vec2 uv;uniform sampler2D level0;uniform sampler2D level1;uniform sampler2D level2;uniform sampler2D background;uniform vec3 weights;uniform vec4 glow_clip;
uniform vec4 crop;uniform vec2 screen;out vec4 result;
vec3 linearize(vec3 c){return mix(c/12.92,pow((c+0.055)/1.055,vec3(2.4)),step(vec3(0.04045),c));}
vec3 gamma(vec3 c){return mix(c*12.92,1.055*pow(c,vec3(1./2.4))-0.055,step(vec3(0.0031308),c));}
void main(){vec4 bg=texture(background,uv);vec2 tc=(uv*screen-crop.xy)/crop.zw;vec2 lo=max(gl_FragCoord.xy-.5,glow_clip.xy),hi=min(gl_FragCoord.xy+.5,glow_clip.zw);float coverage=clamp(hi.x-lo.x,0.,1.)*clamp(hi.y-lo.y,0.,1.);vec3 light=(texture(level0,tc).rgb*weights.x+texture(level1,tc).rgb*weights.y+texture(level2,tc).rgb*weights.z)*coverage;result=vec4(gamma(clamp(linearize(bg.rgb)+light,0.,1.)),bg.a);}
"#;
