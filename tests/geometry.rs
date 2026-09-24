//! The package's polyline arithmetic against a revision cloud -- a closed
//! LWPOLYLINE whose every segment is a 110-degree arc, over chords of
//! differing lengths -- whose length and area are checked here against a
//! computation that does not go through this crate.

use iron_pack_cad::geom;
use uncad_model::model::PolylineVertex;
use uncad_model::Point2D;

/// Twenty-five vertices on an ellipse, unevenly spaced, counter-clockwise,
/// each segment bulging outward as a 110-degree arc.
fn revision_cloud() -> Vec<PolylineVertex> {
    let n = 25;
    let bulge = (110f64.to_radians() / 4.0).tan();
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * i as f64 / n as f64 + 0.08 * (i as f64).sin();
            PolylineVertex {
                bulge,
                ..PolylineVertex::straight(Point2D {
                    x: 60.0 * t.cos(),
                    y: 35.0 * t.sin(),
                })
            }
        })
        .collect()
}

#[test]
fn the_revision_clouds_length_is_the_sum_of_its_arcs() {
    let cloud = revision_cloud();
    // Independently: chord c and bulge b give r = c (1 + b^2) / (4 |b|) and
    // the arc length r * |4 atan b|, summed over the 25 closing segments.
    let n = cloud.len();
    let mut expected = 0.0;
    for i in 0..n {
        let (a, b) = (cloud[i].point, cloud[(i + 1) % n].point);
        let chord = (b.x - a.x).hypot(b.y - a.y);
        let bulge = cloud[i].bulge.abs();
        let radius = chord * (1.0 + bulge * bulge) / (4.0 * bulge);
        expected += radius * 4.0 * bulge.atan();
    }
    let length = geom::polyline_length(&cloud, true);
    assert!(
        (length - expected).abs() < 1e-9 * expected,
        "{length} vs {expected}"
    );
    // Every segment is a 110-degree arc, so the ratio of arc length to
    // chord is the same for all of them: theta / (2 sin(theta / 2)).
    let chords: Vec<PolylineVertex> = cloud
        .iter()
        .map(|v| PolylineVertex::straight(v.point))
        .collect();
    let chord_length = geom::polyline_length(&chords, true);
    let theta = 110f64.to_radians();
    let ratio = theta / (2.0 * (theta / 2.0).sin());
    assert!(
        (length / chord_length - ratio).abs() < 1e-9,
        "{length} / {chord_length} vs {ratio}"
    );
    // A cloud bulging outward encloses its vertex polygon and, on every
    // chord, the circular segment the arc cuts off: r^2 (theta - sin theta)
    // / 2.
    let area = geom::polyline_area(&cloud, true).expect("closed outline");
    let polygon = geom::polyline_area(&chords, true).expect("closed outline");
    let segments: f64 = (0..n)
        .map(|i| {
            let (a, b) = (cloud[i].point, cloud[(i + 1) % n].point);
            let chord = (b.x - a.x).hypot(b.y - a.y);
            let radius = chord / (2.0 * (theta / 2.0).sin());
            radius * radius * (theta - theta.sin()) / 2.0
        })
        .sum();
    assert!(area > polygon, "{area} vs polygon {polygon}");
    assert!(
        (area - (polygon + segments)).abs() < 1e-9 * area,
        "{area} vs {polygon} + {segments}"
    );
}
