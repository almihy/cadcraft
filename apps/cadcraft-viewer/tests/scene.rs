//! The viewer's scene on a small drawing made here, and (with `CADCRAFT_VIEWER_DWG` set, ignored
//! otherwise) on a real one: read, pack, open from the pack, build, hide layers.

use cadcraft_doc::{Common, Drawing, EntityKind, Layer, Line, Space};
use cadcraft_geom::{Vec2, Vec3};
use cadcraft_viewer::scene::Scene;

fn two_layers() -> Drawing {
    let mut d = Drawing::new_imperial();
    d.layers.push(Layer { name: "WALLS".into(), ..Layer::default() });
    d.layers.push(Layer { name: "FURN".into(), on: false, ..Layer::default() });
    let add = |d: &mut Drawing, layer: &str, y: f64| {
        let line = Line { a: Vec3::new(0.0, y, 0.0), b: Vec3::new(10.0, y, 0.0) };
        let c = Common { layer: layer.into(), ..Common::default() };
        assert!(d.add(&Space::Model, c, EntityKind::Line(line)).is_ok());
    };
    add(&mut d, "WALLS", 0.0);
    add(&mut d, "FURN", 5.0);
    d
}

#[test]
fn a_layer_off_in_the_file_starts_hidden_and_can_be_shown() {
    let mut s = Scene::new(two_layers(), "t.dxf", 0.0, 800.0);
    let furn = s.layers().into_iter().find(|l| l.name == "FURN").map(|l| l.visible);
    assert_eq!(furn, Some(false));
    // Both lines are in the list (built with every layer on); one is shown.
    assert_eq!(s.list.prims.len(), 2);
    assert_eq!(s.shown(false).1.count(), 1);
    assert!(!s.set_visible(&["furn".into()], true), "model space needs no new build");
    assert_eq!(s.shown(false).1.count(), 2);
    let mesh = s.mesh(false, Vec2::ZERO, cadcraft_color::Rgb(255, 255, 255));
    assert_eq!(mesh.line_vertices(), 4);
    s.set_visible(&["WALLS".into(), "FURN".into()], false);
    assert_eq!(s.mesh(false, Vec2::ZERO, cadcraft_color::Rgb(255, 255, 255)).line_vertices(), 0);
}

#[test]
fn a_packed_drawing_opens_like_its_file() {
    let d = two_layers();
    let pack = cadcraft_index::pack(&d).unwrap_or_default();
    let (back, _) = Scene::read(&pack, "t.dwg").unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(back.model.len(), 2);
    assert_eq!(back.layers.len(), d.layers.len());
}

/// `CADCRAFT_VIEWER_DWG=plan.dwg cargo test -p cadcraft-viewer --release -- --ignored --nocapture`
#[test]
#[ignore = "needs a real drawing"]
fn real_drawing_timings() {
    let Ok(path) = std::env::var("CADCRAFT_VIEWER_DWG") else { return };
    let bytes = std::fs::read(&path).unwrap_or_default();
    let t = std::time::Instant::now();
    let (d, _) = Scene::read(&bytes, &path).unwrap_or_else(|e| panic!("{e}"));
    let read = t.elapsed();
    let pack = cadcraft_index::pack(&d).unwrap_or_default();
    let _ = std::fs::write(format!("{path}.pack"), &pack);
    let t = std::time::Instant::now();
    let (d2, _) = Scene::read(&pack, "x").unwrap_or_else(|e| panic!("{e}"));
    let unpack = t.elapsed();
    let t = std::time::Instant::now();
    let s = Scene::new(d2, "x", 0.0, 1400.0);
    let built = t.elapsed();
    let mut s = s;
    let ext = s.list.bounds;
    let seen = cadcraft_geom::Bounds2::from_points([ext.center(), ext.center() + (ext.max - ext.min) * 0.03]);
    let t = std::time::Instant::now();
    s.build_detail(&seen, s.built_scale * 30.0);
    let detail = t.elapsed();
    println!(
        "read {read:?}, pack {} MB, unpack {unpack:?}, build {built:?}, prims {}, items {}, detail {detail:?} ({} prims)",
        pack.len() / 1_000_000,
        s.list.prims.len(),
        d.model.len(),
        s.detail.as_ref().map_or(0, |d| d.list.prims.len())
    );
}
