use super::*;

#[test]
fn scenes_collect_independent_regions_and_bound_bad_geometry() {
    let bounds = Rect::from_min_size(Pos2::ZERO, Vec2::new(850.0, 530.0));
    let mut frame = Frame::new(egui::ViewportId::ROOT, bounds, 1.5);
    let mut field = Scene::default();
    field.points(
        (0..200).map(|i| Pos2::new(i as f32, 100.0)),
        1.0,
        Color32::BLUE,
        Some(Emission::new(Color32::BLUE, 2.0)),
    );
    field.disc(Pos2::new(f32::NAN, 0.0), 1.0, Color32::WHITE, None);
    field.path([Pos2::ZERO], Stroke::new(1.0, Color32::WHITE), None);
    assert_eq!(field.marks.len(), 200);
    let mut curve = Scene::default();
    curve.path(
        (0..513).map(|i| Pos2::new(i as f32, 250.0)),
        Stroke::new(1.0, Color32::BLUE),
        Some(Emission::new(Color32::BLUE, 1.0)),
    );
    frame
        .submit(
            egui::Id::unique("field"),
            1,
            bounds,
            bounds.expand(12.0),
            field,
        )
        .unwrap();
    frame
        .submit(egui::Id::unique("curve"), 7, bounds, bounds, curve)
        .unwrap();
    frame
        .submit(
            egui::Id::unique("empty"),
            0,
            Rect::NOTHING,
            bounds,
            Scene::default(),
        )
        .unwrap();
    assert_eq!(frame.regions.len(), 2);
    assert_eq!(frame.regions[0].glow_clip, bounds);
    assert_eq!(frame.regions[1].revision, 7);
}

#[test]
fn plot_roundtrips_units_at_different_sizes_and_hit_testing_uses_nodes() {
    for scale in [0.8, 1.0, 1.5] {
        let plot = Plot {
            rect: Rect::from_min_size(Pos2::new(24.0, 12.0), Vec2::new(400.0, 200.0) * scale),
            x: Axis::logarithmic(20.0, 20_000.0).unwrap(),
            y: Axis::linear(-18.0, 18.0).unwrap(),
        };
        for p in [[20.0, -18.0], [1000.0, 6.0], [20_000.0, 18.0]] {
            let q = plot.value(plot.point(p[0], p[1]));
            assert!((p[0] - q[0]).abs() / p[0] < 1e-5);
            assert!((p[1] - q[1]).abs() < 1e-4);
        }
        assert_eq!(
            plot.hit(plot.point(1000.0, 6.0), &[[1000.0, 6.0]], 7.0),
            Some(0)
        );
        assert_eq!(
            plot.hit(plot.point(2000.0, 6.0), &[[1000.0, 6.0]], 7.0),
            None
        );
    }
    assert!(Axis::logarithmic(0.0, 1.0).is_none());
    assert!(Axis::linear(1.0, 1.0).is_none());
}
