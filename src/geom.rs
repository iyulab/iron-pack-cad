//! The arithmetic the package's records carry and the model does not: a
//! polyline's length and area with its bulge arcs, whether an outline
//! crosses itself, a polygon's centroid, and the map from an entity's own
//! plane to the world.
//!
//! The model states a drawing and the coordinate arithmetic the format
//! defines -- a block reference's placement, a bulge's arc, a curve's
//! points; this module takes a polyline's segments from it
//! ([`uncad_model::bulge::segments`]) rather than working them out again.
//! Lengths and areas are this consumer's derivations, and every one of them
//! is exact arithmetic on what the file states, not an estimate.

use uncad_model::bulge::{segments, BulgeArc, Segment};
use uncad_model::model::{InsertEntity, Point2D, Point3D, PolylineVertex};
use uncad_model::{Affine2, Ocs, Tables};

/// How long the arc a bulge describes is.
fn arc_length(arc: &BulgeArc) -> f64 {
    arc.radius * arc.sweep.abs()
}

/// The area between a bulge's arc and its chord (the circular segment),
/// always positive.
fn segment_area(arc: &BulgeArc) -> f64 {
    let theta = arc.sweep.abs();
    arc.radius * arc.radius / 2.0 * (theta - theta.sin())
}

fn segment_length(s: &Segment) -> f64 {
    match &s.arc {
        Some(arc) => arc_length(arc),
        None => (s.to.x - s.from.x).hypot(s.to.y - s.from.y),
    }
}

/// The polyline's length (its perimeter when closed), arcs included.
pub fn polyline_length(vertices: &[PolylineVertex], closed: bool) -> f64 {
    segments(vertices, closed).map(|s| segment_length(&s)).sum()
}

/// The signed area a polyline encloses, arcs included: the shoelace area of
/// its vertices plus each arc's circular segment, added when the arc bulges
/// outward and subtracted when it bulges inward. Positive for a
/// counter-clockwise outline. An open polyline is closed by a straight
/// segment from its last vertex back to its first for this purpose: the
/// bulge stored on the last vertex (AutoCAD keeps one there, e.g. after
/// BREAK or TRIM) applies to no segment, exactly as in
/// the model's [`segments`]. Self-intersecting outlines give a value with no
/// geometric meaning; see [`is_simple`].
pub fn polyline_signed_area(vertices: &[PolylineVertex], closed: bool) -> f64 {
    let mut segments: Vec<Segment> = segments(vertices, closed).collect();
    if !closed && vertices.len() >= 2 {
        segments.push(Segment {
            from: vertices[vertices.len() - 1].point,
            to: vertices[0].point,
            arc: None,
        });
    }
    // Three straight segments are the fewest that can enclose anything --
    // but two *arcs* can. A closed two-vertex polyline with bulges is
    // exactly what AutoCAD's DONUT command writes, and two bulges of 1
    // describe a full circle. The shoelace terms of such a shape cancel to
    // zero, so what is left is the two circular-segment terms, which are
    // its area. Fewer than three segments with no arc among them still
    // encloses nothing.
    if segments.len() < 3 && segments.iter().all(|s| s.arc.is_none()) {
        return 0.0;
    }
    let mut area = 0.0;
    for Segment { from, to, arc } in &segments {
        area += (from.x * to.y - to.x * from.y) / 2.0;
        if let Some(arc) = arc {
            // A counter-clockwise arc bulges to the right of travel, which
            // is outward for a counter-clockwise outline and inward for a
            // clockwise one -- so adding a signed term does the right thing
            // for both orientations.
            area += segment_area(arc) * arc.sweep.signum();
        }
    }
    area
}

/// The enclosed area, unsigned, when the polyline can enclose one at all:
/// three or more vertices, or a closed pair (two arcs, AutoCAD's DONUT).
/// `None` otherwise.
pub fn polyline_area(vertices: &[PolylineVertex], closed: bool) -> Option<f64> {
    let enclosing = vertices.len() >= 3 || (closed && vertices.len() == 2);
    enclosing.then(|| polyline_signed_area(vertices, closed).abs())
}

/// Whether the straight-segment outline through `vertices` (closed) has no
/// two non-adjacent segments crossing. Quadratic in the vertex count --
/// callers bound it (the package tests at most 2000 vertices); arcs are
/// treated as their chords.
pub fn is_simple(vertices: &[Point2D]) -> bool {
    let vertices = if vertices.len() > 2 && vertices[0] == vertices[vertices.len() - 1] {
        &vertices[..vertices.len() - 1]
    } else {
        vertices
    };
    let n = vertices.len();
    if n < 4 {
        return true;
    }
    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let (a, b) = (vertices[i], vertices[(i + 1) % n]);
            let (c, d) = (vertices[j], vertices[(j + 1) % n]);
            if segments_cross(a, b, c, d) {
                return false;
            }
        }
    }
    true
}

fn segments_cross(a: Point2D, b: Point2D, c: Point2D, d: Point2D) -> bool {
    let orient =
        |p: Point2D, q: Point2D, r: Point2D| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (o1, o2, o3, o4) = (
        orient(a, b, c),
        orient(a, b, d),
        orient(c, d, a),
        orient(c, d, b),
    );
    (o1 > 0.0) != (o2 > 0.0)
        && (o3 > 0.0) != (o4 > 0.0)
        && o1 != 0.0
        && o2 != 0.0
        && o3 != 0.0
        && o4 != 0.0
}

/// Area-weighted centroid of a polygon's vertices (straight segments); the
/// vertices' mean when they enclose no area.
pub fn polygon_centroid(vertices: &[Point2D]) -> Point2D {
    let n = vertices.len();
    if n == 0 {
        return Point2D::default();
    }
    let (mut cx, mut cy, mut area) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (a, b) = (vertices[i], vertices[(i + 1) % n]);
        let cross = a.x * b.y - b.x * a.y;
        cx += (a.x + b.x) * cross;
        cy += (a.y + b.y) * cross;
        area += cross;
    }
    if area.abs() < 1e-12 {
        let sx: f64 = vertices.iter().map(|p| p.x).sum();
        let sy: f64 = vertices.iter().map(|p| p.y).sum();
        return Point2D {
            x: sx / n as f64,
            y: sy / n as f64,
        };
    }
    Point2D {
        x: cx / (3.0 * area),
        y: cy / (3.0 * area),
    }
}

/// Whether `p` is inside the polygon `vertices` (even-odd rule).
pub fn point_in_polygon(p: Point2D, vertices: &[Point2D]) -> bool {
    let n = vertices.len();
    if n == 0 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (vertices[i], vertices[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let x = (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x;
            if p.x < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// The map from an entity's own plane -- the object coordinate system whose
/// Z axis is `extrusion`, at height `elevation` in it -- to the world's (x,
/// y): the DXF reference's arbitrary axis algorithm, seen from above. The
/// identity for the usual normal (0, 0, 1), a mirror across the y axis for
/// (0, 0, -1).
///
/// The axes are the model's own [`Ocs`], so the package and every other
/// consumer of the model agree on the algorithm to the bit. A plane
/// parallel to the world's maps exactly; a tilted one is seen from above,
/// the plane's height moving it along the projection of its normal. A
/// normal that is not a direction places the entity as (0, 0, 1) would.
pub fn plane_to_world(extrusion: Point3D, elevation: f64) -> Affine2 {
    let plane = Ocs::of(extrusion).unwrap_or(Ocs::WORLD);
    let (x, y, z) = (plane.x_axis(), plane.y_axis(), plane.z_axis());
    Affine2 {
        a: x.x,
        b: x.y,
        c: y.x,
        d: y.y,
        e: elevation * z.x,
        f: elevation * z.y,
    }
}

/// Where an INSERT puts its block's entities in the world's XY: the
/// model's [`InsertEntity::world_transform`] for a reference in a plane
/// parallel to the world's, and otherwise its placement in its own plane
/// with that plane seen from above ([`plane_to_world`]). The block's base
/// point, which the placement puts on the insertion point, is looked up in
/// `tables`; a block the tables do not hold has nothing to place, and is
/// taken as based at the origin.
pub fn insert_to_world(insert: &InsertEntity, tables: &Tables) -> Affine2 {
    let base = block_base_point(insert, tables);
    insert.world_transform(base).unwrap_or_else(|| {
        insert
            .transform(base)
            .then(&plane_to_world(insert.extrusion, insert.insertion_point.z))
    })
}

/// The base point of the block `insert` refers to -- the point its placement
/// puts on the insertion point -- or the origin for a block the tables do not
/// hold.
pub fn block_base_point(insert: &InsertEntity, tables: &Tables) -> Point3D {
    insert
        .block_name
        .resolved()
        .and_then(|name| tables.block_records.get(name))
        .map_or_else(Point3D::default, |block| block.base_point)
}

/// Whether `m` keeps shapes: a rotation, a uniform scale and possibly one
/// mirror. A circle stays a circle through it, a bulge stays a bulge (its
/// sign turned by a mirror), and lengths scale by `sqrt(|det|)`.
pub fn is_similarity(m: &Affine2) -> bool {
    let la = m.a.hypot(m.b);
    let lb = m.c.hypot(m.d);
    let dot = m.a * m.c + m.b * m.d;
    la > 0.0 && lb > 0.0 && dot.abs() <= 1e-9 * la * lb && (la - lb).abs() <= 1e-9 * la.max(lb)
}

/// `vertices` taken through `m`, when `m` is a similarity: the points
/// moved, a bulge's sign turned by a mirror (an arc that ran
/// counter-clockwise runs clockwise in a mirrored plane), the widths scaled.
/// `None` for a map that does not keep an arc an arc.
pub fn map_polyline(vertices: &[PolylineVertex], m: &Affine2) -> Option<Vec<PolylineVertex>> {
    if !is_similarity(m) {
        return None;
    }
    let flip = if m.determinant() < 0.0 { -1.0 } else { 1.0 };
    let scale = m.determinant().abs().sqrt();
    Some(
        vertices
            .iter()
            .map(|v| PolylineVertex {
                point: m.apply(v.point),
                bulge: v.bulge * flip,
                start_width: v.start_width * scale,
                end_width: v.end_width * scale,
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2D {
        Point2D { x, y }
    }

    fn v(x: f64, y: f64, bulge: f64) -> PolylineVertex {
        PolylineVertex {
            point: p(x, y),
            bulge,
            start_width: 0.0,
            end_width: 0.0,
        }
    }

    fn vs(points: &[(f64, f64)], bulges: &[f64]) -> Vec<PolylineVertex> {
        points
            .iter()
            .enumerate()
            .map(|(i, (x, y))| v(*x, *y, bulges.get(i).copied().unwrap_or(0.0)))
            .collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6
    }

    #[test]
    fn the_design_documents_worked_example() {
        // A 100 x 50 rectangle whose right edge is a 90-degree arc.
        let vertices = vs(
            &[(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (0.0, 50.0)],
            &[0.0, 0.41421356, 0.0, 0.0],
        );
        let all: Vec<Segment> = segments(&vertices, true).collect();
        assert_eq!(all.len(), 4);
        let arc = all[1].arc.expect("second segment is the arc");
        assert!(
            close(arc.center.x, 75.0) && close(arc.center.y, 25.0),
            "{arc:?}"
        );
        assert!(close(arc.radius, 35.355339), "{}", arc.radius);
        assert!(close(arc.sweep.to_degrees(), 90.0), "{}", arc.sweep);
        assert!(close(arc_length(&arc), 55.536037), "{}", arc_length(&arc));
        assert!(
            close(segment_area(&arc), 356.747702),
            "{}",
            segment_area(&arc)
        );
        assert!(close(polyline_length(&vertices, true), 305.536037));
        assert!(close(polyline_area(&vertices, true).unwrap(), 5356.747702));
        // The same outline clockwise: same area, negative sign. Reversed,
        // the arc is the segment leaving vertex 1 ((100,50) down to
        // (100,0)) and must bulge to the left of travel, i.e. negatively.
        let cw = vs(
            &[(0.0, 50.0), (100.0, 50.0), (100.0, 0.0), (0.0, 0.0)],
            &[0.0, -0.41421356, 0.0, 0.0],
        );
        assert!(close(polyline_signed_area(&cw, true), -5356.747702));
        let points: Vec<Point2D> = vertices.iter().map(|v| v.point).collect();
        assert!(is_simple(&points));
    }

    #[test]
    fn bulges_beyond_a_semicircle_put_the_centre_on_the_bulge_side() {
        // A 270-degree arc from (0,0) to (10,0): bulge = tan(67.5 deg).
        let bulge = (270f64 / 4.0).to_radians().tan();
        let arc = BulgeArc::between(p(0.0, 0.0), p(10.0, 0.0), bulge).unwrap();
        assert!(close(arc.sweep.to_degrees(), 270.0));
        // Centre below the chord (the arc bulges to the right of travel,
        // i.e. downward, and past a semicircle the centre is on that side):
        // (5, -5), radius 5 sqrt(2).
        assert!(
            close(arc.center.x, 5.0) && close(arc.center.y, -5.0),
            "{arc:?}"
        );
        assert!(close(arc.radius, 5.0 * 2f64.sqrt()), "{}", arc.radius);
    }

    #[test]
    fn open_polylines_and_degenerate_input() {
        let vertices = vs(&[(0.0, 0.0), (3.0, 4.0)], &[]);
        assert!(close(polyline_length(&vertices, false), 5.0));
        assert_eq!(polyline_area(&vertices, false), None, "two open vertices");
        // A closing vertex that repeats the first adds a segment of no
        // length, which changes neither the length nor the area -- the file's
        // vertices are taken as written.
        let repeated = vs(&[(0.0, 0.0), (4.0, 0.0), (4.0, 3.0), (0.0, 0.0)], &[]);
        assert!(close(polyline_area(&repeated, true).unwrap(), 6.0));
        assert!(close(polyline_length(&repeated, true), 12.0));
    }

    #[test]
    fn an_open_polylines_last_bulge_does_not_bend_the_closing_segment() {
        // The right triangle (0,0) -> (10,0) -> (10,10), open, with a bulge
        // left on its last vertex. Closed by a straight segment the area is
        // 10 * 10 / 2 = 50, and the last bulge applies to nothing. Bent, the
        // closing chord (10,10) -> (0,0) of length 10 sqrt(2) would carry a
        // semicircle of radius 5 sqrt(2), adding or taking pi * 50 / 2 =
        // 78.539816.
        for bulge in [1.0, -1.0, 669.19] {
            let vertices = vs(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], &[0.0, 0.0, bulge]);
            assert!(
                close(polyline_signed_area(&vertices, false), 50.0),
                "bulge {bulge}: {}",
                polyline_signed_area(&vertices, false)
            );
            // Length is unaffected either way: two straight sides.
            assert!(close(polyline_length(&vertices, false), 20.0));
        }
        // Flagged closed, the same bulge is the real closing segment.
        let vertices = vs(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)], &[0.0, 0.0, 1.0]);
        assert!(close(
            polyline_signed_area(&vertices, true),
            50.0 + std::f64::consts::PI * 50.0 / 2.0
        ));
        // An open polyline drawn back to its first vertex: the bulge on the
        // vertex before the repeat is a real segment, the repeat's own is
        // not. The third segment (10,10) -> (0,0) with bulge -1 is a
        // semicircle inside the triangle, so 50 - 78.539816; the 5.0 on the
        // repeated vertex must count for nothing.
        let back_home = vs(
            &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 0.0)],
            &[0.0, 0.0, -1.0, 5.0],
        );
        assert!(close(
            polyline_signed_area(&back_home, false),
            50.0 - std::f64::consts::PI * 50.0 / 2.0
        ));
    }

    #[test]
    fn self_intersection_is_detected() {
        let bowtie = [p(0.0, 0.0), p(10.0, 10.0), p(10.0, 0.0), p(0.0, 10.0)];
        assert!(!is_simple(&bowtie));
        let square = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
        assert!(is_simple(&square));
        // The same square with its closing vertex repeated.
        let repeated = [
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 10.0),
            p(0.0, 10.0),
            p(0.0, 0.0),
        ];
        assert!(is_simple(&repeated));
    }

    #[test]
    fn a_closed_two_vertex_bulged_polyline_encloses_its_real_area() {
        // AutoCAD's DONUT: two vertices 100 apart with bulges of 1 each is
        // a circle of radius 50, so the area is pi * 50^2 = 7853.981634 and
        // the perimeter is 2 * pi * 50 = 314.159265. Both numbers come from
        // the circle, not from this crate.
        let donut = vs(&[(0.0, 0.0), (100.0, 0.0)], &[1.0, 1.0]);
        let area = polyline_area(&donut, true).unwrap();
        assert!(close(area, std::f64::consts::PI * 2500.0), "{area}");
        assert!(close(
            polyline_length(&donut, true),
            std::f64::consts::TAU * 50.0
        ));
        // Orientation still reads: two negative bulges trace the same
        // circle clockwise.
        assert!(polyline_signed_area(&donut, true) > 0.0);
        let clockwise = vs(&[(0.0, 0.0), (100.0, 0.0)], &[-1.0, -1.0]);
        assert!(polyline_signed_area(&clockwise, true) < 0.0);
        // Two *straight* segments still enclose nothing -- there and back
        // along the same line.
        let straight = vs(&[(0.0, 0.0), (100.0, 0.0)], &[0.0, 0.0]);
        assert!(close(polyline_area(&straight, true).unwrap(), 0.0));
        // One bulge of 1, one of 0: a semicircle closed by its diameter,
        // pi * 50^2 / 2.
        let half = vs(&[(0.0, 0.0), (100.0, 0.0)], &[1.0, 0.0]);
        assert!(close(
            polyline_area(&half, true).unwrap(),
            std::f64::consts::PI * 1250.0
        ));
    }

    #[test]
    fn centroids_and_containment() {
        let square = [p(0.0, 0.0), p(4.0, 0.0), p(4.0, 2.0), p(0.0, 2.0)];
        let c = polygon_centroid(&square);
        assert!(close(c.x, 2.0) && close(c.y, 1.0), "{c:?}");
        assert!(point_in_polygon(p(1.0, 1.0), &square));
        assert!(!point_in_polygon(p(5.0, 1.0), &square));
        // No area: the mean of the vertices.
        let line = [p(0.0, 0.0), p(10.0, 0.0)];
        let c = polygon_centroid(&line);
        assert!(close(c.x, 5.0) && close(c.y, 0.0), "{c:?}");
    }

    #[test]
    fn a_mirrored_plane_turns_x_and_the_bulges_sign() {
        // (0, 0, -1): the arbitrary axis algorithm takes the OCS x axis to
        // the world's -x (Wy x N) and leaves y alone.
        let mirror = plane_to_world(
            Point3D {
                x: 0.0,
                y: 0.0,
                z: -1.0,
            },
            0.0,
        );
        let q = mirror.apply(p(3.0, 4.0));
        assert!(close(q.x, -3.0) && close(q.y, 4.0), "{q:?}");
        assert!(is_similarity(&mirror) && mirror.determinant() < 0.0);
        // A counter-clockwise half circle in the mirrored plane runs
        // clockwise in the world: same length and area, orientation turned.
        let ocs = vs(&[(0.0, 0.0), (10.0, 0.0)], &[1.0, 1.0]);
        let world = map_polyline(&ocs, &mirror).unwrap();
        assert!(world.iter().all(|v| v.bulge == -1.0));
        assert!(close(
            polyline_length(&world, true),
            polyline_length(&ocs, true)
        ));
        assert!(close(
            polyline_signed_area(&world, true),
            -polyline_signed_area(&ocs, true)
        ));
        // The usual normal is the identity, whatever the elevation.
        let flat = plane_to_world(
            Point3D {
                x: 0.0,
                y: 0.0,
                z: 1.0,
            },
            7.0,
        );
        assert_eq!(flat, Affine2::IDENTITY);
        // A tilted plane seen from above is no similarity: an arc in it is
        // an ellipse in the plan.
        let tilted = plane_to_world(
            Point3D {
                x: 1.0,
                y: 0.0,
                z: 1.0,
            },
            0.0,
        );
        assert!(!is_similarity(&tilted));
        assert!(map_polyline(&ocs, &tilted).is_none());
    }
}

#[cfg(test)]
mod revision_cloud {
    // The package's polyline arithmetic against a revision cloud -- a closed
    // LWPOLYLINE whose every segment is a 110-degree arc, over chords of
    // differing lengths -- whose length and area are checked here against a
    // computation that does not go through this crate.

    use super::*;

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
        let length = polyline_length(&cloud, true);
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
        let chord_length = polyline_length(&chords, true);
        let theta = 110f64.to_radians();
        let ratio = theta / (2.0 * (theta / 2.0).sin());
        assert!(
            (length / chord_length - ratio).abs() < 1e-9,
            "{length} / {chord_length} vs {ratio}"
        );
        // A cloud bulging outward encloses its vertex polygon and, on every
        // chord, the circular segment the arc cuts off: r^2 (theta - sin theta)
        // / 2.
        let area = polyline_area(&cloud, true).expect("closed outline");
        let polygon = polyline_area(&chords, true).expect("closed outline");
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
}
