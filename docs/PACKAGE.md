# The package

`export_package` turns a drawing, given as the `uncad-model` entity model and
optionally its header, into a directory that a language model or a vision
model can read. The images say *where* things are; the JSON records say
*what* they are, with the exact numbers a picture cannot give. The images are
drawn by `iron-render-cad` (called "the renderer" below). Every record
points at the pixels it is drawn in, and every image publishes the map from
its pixels back to drawing coordinates.

This document describes that directory as a contract, and the principles it
is built on. A package describes itself as well: `manifest.json` carries a
`legend` that defines each vocabulary the records use and a `guidance`
string that says how to look something up.

## Principles

**Numbers come from the records, never from pixels.** Lengths, areas,
dimension values, counts and texts are computed from the model's
coordinates and written into JSON. The images carry spatial context only.
A reader that needs a value reads it from a record; a reader looking at a
tile finds the record ids on it in the tile's sidecar.

**The same drawing and options give the same bytes.** Every file of the
package, `report.json` included, is byte-identical on a second run with the
same input, options and library version. Output order never depends on a
hash collection's iteration order, nothing records a clock, images drawn in
parallel are written in a fixed order, and the bundled font makes text
layout independent of the fonts installed on the machine.

**Nothing is left out silently.** Every entity the picture does not show is
listed in `report.json` with a reason. Every list that is capped says so,
beside an exact count: a truncated sidecar keeps exact `counts`, the
hidden-entity list states `handles_limit` and `handles_truncated`, and
`frames_dropped` is accompanied by `frames_dropped_total`. A tile that is
planned but not written is listed in `tiles.json` with its reason.

**Where a drawing states two facts, both are reported.** A dimension stores
a measurement and also has definition points that give one. They can
disagree -- a file may carry a stale or foreign value. The package writes
both, with their difference, and chooses neither; which to trust is the
reader's decision. A value is only withheld when it is not a value at all.

**Stated, not guessed.** Units come from the header's `$INSUNITS`; without
it the unit is `du` (drawing units) and `to_mm` is null. A derived value
that is approximate says so (`confidence`, `bbox_confidence`), with `why`
where a specific reason is recorded. A geometry or region record's
`confidence` never exceeds the model's own confidence in the entity.

**Pixel boxes are clipped; world boxes are whole.** A record's `bbox` is its
full extent in drawing units. Its pixel box on an image is clipped to that
image, so it never names pixels the image does not have; the rest of the
record is on the other images it lists.

**The profile sets the image sizes.** Image and tile sizes follow the budget
of the model family the package is for, not a free-form option.

| Profile | Overview edge | Overview patches | Tile | Overlap | Patch |
|---|---|---|---|---|---|
| `claude` (default) | 1568 px | 1568 | 1092 px | 224 px | 28 px |
| `claude-hires` | 2576 px | 4784 | 1932 px | 392 px | 28 px |
| `openai-patch` | 2048 px | 4096 | 1600 px | 320 px | 32 px |

## Files

```text
README.txt                  reading order
manifest.json               what is here: read first
drawing.json                header, units, layers, block definitions, counts
overview.png                the whole crop, fitted to the profile
frames/fN/overview.png      one per frame, when the drawing has several
frames/fN/tiles/z{z}/r{rr}_c{cc}.png   tiles
frames/fN/tiles/z{z}/r{rr}_c{cc}.json  their sidecars
tiles.json                  every planned tile, written or empty
sheets.json                 the paper layouts
sheets/<layout>/overview.png   one image per layout
texts.json                  text records
dimensions.json             dimension records
geometry.json               every other visible entity
regions.json                closed outlines with an area
blocks.json                 block instances
strings.json                normalized string -> record ids
report.json                 what was left out, and why
drawing.svg                 optional; a tool input, not for reading
entities.json               optional; the whole model, a tool input
```

**`manifest.json`** holds the source (`name`, `format`, `acadver`,
`version`, `codepage`), `units`, `profile`, `crop`, `overview`, `frames`,
`frames_dropped`, `sheets`, `legibility`, `counts`, `capabilities`,
`legend`, `guidance`, `files` (every file with its kind and byte count),
`shard_index` and `warnings`. `capabilities` summarizes what the records can
answer: how many dimensions carry a stored and a point-derived value, how
exact the region areas are, whether text boxes were measured, which fonts
were used, and whether paper layouts and their texts are present.

**`drawing.json`** holds the header as given, the units, the reader's
diagnostics, every layer with its state (`on`, `frozen`, `locked`, `plot`,
colour, linetype, lineweight) and entity count, the block definitions
(`entity_count`, `count_by_layer`, `attrib_tags`, `anonymous`, and the ids
of their instances), and entity counts by type.

**`report.json`** lists the entities the picture leaves out and why, the
hidden entities by reason, unsupported entity types, empty blocks,
unresolved block references, the ARCs not drawn because their start and
end angles are equal (the format does not say whether such an arc is the
whole circle or nothing), the renderer's robustness limits that were
engaged, and the warnings.

## Conventions

- Every JSON file carries `"$schema": "iron-pack-cad/1"`. `manifest.json`,
  `drawing.json` and every record file also carry a `units` block
  `{name, insunits, to_mm, source}`.
- A record's `id` is the model's entity reference ID in decimal, or a path
  `<insert>/<child>` for a text drawn inside a block. Its `handle` is the
  file's own handle, in hexadecimal, when the entity came from a file.
- Points are `[x, y]`; boxes are `[x0, y0, x1, y1]`. Every point in the
  package is planar. Angles are in degrees. Pixel coordinates are integers,
  absolute, with y down.
- Coordinates are rounded to the drawing's `$LUPREC` (clamped to 3..12), but
  never to fewer decimals than put one unit in the last place at a
  thousandth of a pixel at the deepest zoom level the package can reach.
  Derived values carry two decimals more.
- Record files are written in id order, ids compared as numbers. Above the
  shard size (96 KB by default) a file is split into `name.001.json`,
  `name.002.json`, and so on. `manifest.shard_index` lists every record file
  with its kind and `first_key`/`last_key`: the numeric bounds of the first
  number of its ids. An id does not state its kind, and one id can be in two
  kinds -- a closed polyline is both a geometry and a region record -- so a
  lookup checks every entry whose bounds contain it.
- `manifest.json` is written last. A directory that holds one is a finished
  package. Exporting into a directory first removes every file the previous
  manifest listed, and the directories under `frames/` and `sheets/` that
  this leaves empty; nothing else is touched.

## Images

### Crop and overview

The crop is the renderer's framing of model space. By default (`Auto`) it
covers every entity except the few outliers the renderer sets aside -- an
entity far larger than the rest, or far away from it -- or the header's
`$EXTMIN/$EXTMAX` when those form a sane rectangle and cover more. `Raw`
frames every entity, `Header` the header's extents, and `Fixed` a given
rectangle. `manifest.crop` names the rule that applied (`content`, `header`,
`raw`, `fixed` or `empty`), the rectangle, the tight content bounds, the
padding and the header's extents, and lists each excluded entity with its
world box and reason (`scale_outlier`, `far_outlier`, `outside_view`).

The overview shows the whole crop. It is fitted to both the profile's edge
and its patch budget, padded by 2 % of the longer side (at least 24 output
pixels), and its pixel size is rounded up to a multiple of the patch size.
The world rectangle then grows on the right and bottom so that pixels and
drawing units stay in exact proportion. Every image publishes `world`,
`ppu` (pixels per unit), and two row-major affines:

```text
world_to_px = [s, 0, -x0*s, 0, -s, y1*s]
px_to_world = [1/s, 0, x0, 0, -1/s, y1]
```

where `s` is `ppu` and `(x0, y1)` is the rectangle's top-left corner.

### Frames

Entities that lie within a small distance of one another (5 % of the crop's
diagonal by default) form one group. The largest group is frame `f0`; a
detached group -- a detail drawn beside the plan -- becomes a frame of its
own when it holds at least 20 entities or one text, up to 8 frames. Each
frame gets its own overview and tile pyramid. A group that is not framed
stays in the overview, its records carry `tiles: []`, and it is listed in
`frames_dropped` with the reason `below_min_entities` or `max_frames`. The
overview itself is never trimmed to a frame.

### Tile pyramid

Level `z1` is twice the frame overview's scale, and each level doubles
again. The depth is set by the
frame's text: the count-weighted median text height is the dominant class,
and the pyramid goes as deep as that class needs to reach the target pixel
height (14 px by default), bounded by `max_levels` (5). The tile budget
(`max_tiles`, 400 written tiles across all frames) is checked per level;
when a level would exceed it, the frame stops at the level above.
`manifest.legibility` reports, per frame, `target_met` (every height class
reaches the target at the deepest level written), `pyramid_complete` (the
budget let the pyramid reach the depth the text asked for) and each height
class with its pixel height at that depth.

A level is cut into square tiles of the profile's size, stepping by the
tile size minus the overlap. The last row and column are shifted inward so
every tile has the full size (or the whole level, when that is smaller than
a tile). Tile ids are `fN/z{z}/r{rr}_c{cc}`, row 0 at
the top. A tile that no visible entity reaches is not written; `tiles.json`
lists it as empty with the reason `no_visible_entity_on_tile`, and lists
every written tile with its byte count and SHA-256.

**Why tiles overlap.** An object cut by one tile's edge lies whole on a
neighbouring tile whenever it spans no more pixels, at that level, than the
overlap, so a label or
a symbol at a seam is always readable somewhere. The same record then
appears in each overlapping tile's sidecar under the same `id`, and its
`tiles` list names all of them, so a repeat is identified by id, not by
comparing boxes.

Each tile is drawn from the entities whose extent reaches it, with a small
margin for strokes and text overhang. Text extents are the measured glyph
boxes, so frames and tiles are culled by what is actually drawn; the crop
of the overview uses the renderer's framing.

### Sidecars

Beside each written tile, a JSON sidecar states the tile's `world`, `ppu`,
both affines, `canvas_origin_px`, `overlap_px`, its written `neighbors`
(`n`, `s`, `w`, `e`), its `parent` and `children` in the pyramid, the layers
present, and the records on it as positional rows whose fields are named
in `columns`: texts (id, pixel box, text), dimensions (id, pixel box,
display, both measurements), blocks, regions (with area) and geometry (with
type). A sidecar is kept within 32 KB. When it would, geometry rows are cut
first, then the other rows, then the layer list, and each cut is flagged
(`records_truncated`, `layers_truncated`). `counts` and `geometry_by_kind`
are never truncated.

### Sheets

Each paper layout is drawn whole, fitted to the profile without padding, as
`sheets/<layout>/overview.png` with image id `sheet:<layout>`. The model is
composited through the layout's viewports. `sheets.json` gives, per sheet,
its paper rectangle in paper units and where it came from (`layout_limits`,
`paper_size`, `stated`, `entities` or `empty`), the plot settings, and each
viewport: its frame, scale, twist, frozen layers, whether the model was
drawn through it (`composited`; the layout's overall frame never is), the
model window, and `model_to_paper`, the affine the model was drawn through.

## Records

Every record carries `id`, `handle`, `layer`, `bbox`, `tiles` (the tile ids
it is on) and `px` (image id to pixel box, for the overview, frame overviews
and tiles).

**Texts** (`texts.json`): TEXT, MTEXT, ATTRIB, TOLERANCE and the text a MULTILEADER points out (`kind` `MULTILEADER`, under the multileader's ID), block contents
included. `text` is the readable string with format codes decoded; `raw` is
the string as written, when it differs. Each record carries `kind`, `tag`
(attributes), `height`, `rotation_deg` (the orientation a reader sees),
`anchor`, `style`, `font_ok`, and `bbox_confidence`: `measured` from the
glyph outlines as the renderer laid them out, or `estimated` at 0.6 em per
character. A dimension's label is part of the dimension record, not a text
record. `space` is `model` or `paper`; a paper-space text -- a title block,
a sheet title -- names its `sheet`, is measured in that layout's paper
units, lists no tiles, and carries a pixel box on the sheet image.

**Dimensions** (`dimensions.json`): `kind`, `dimstyle`,
`measurement_stored` (what the file stores), `measurement_from_points`
(what the definition points give), `delta` (stored minus from points, when
both exist), `unit` (`deg` for angular kinds), and `dimlfac`; both values
are before `DIMLFAC`. A stored value is withheld only when it is not a
measurement: not finite, `-1`, or `0` on any kind but ORDINATE. `display`
is the label, with `display_source` saying where it came from: `suppressed`
(the override is whitespace), `user_text` (the override, `<>` filled in),
`cached_block` (the label the drawing stores), `formatted` (formatted here
by basic rules), or `none`. When the label holds a number formatted here,
`display_value_from` says whether it is the points' value or the stored
one. The record also carries `points` by role, `definition_point`,
`text_at`, `rotation_deg` for a rotated dimension, `ordinate_axis` for an
ordinate, and `arc_symbol` for an arc length -- the style's `DIMARCSYM`,
which is drawn beside the label and never part of `display`. The style
values (`dimlfac`, `arc_symbol`, and those a `formatted` label is written
with) are the style as the dimension sees it: its DIMSTYLE with the
dimension's own overrides applied. A dimension carries no `confidence`.

**Geometry** (`geometry.json`): every other visible entity with an extent,
with `type`, `unit`, key points and closed-form measures -- a line's
`from`, `to` and `length` (the 3D length, with `length_plan` and `dz` only
when it leaves the plane); an arc's centre, radius, angles, sweep and
length; a circle's length and area; a polyline's vertices (with bulges
when it has any), length or perimeter, and for a closed one `area`,
`orientation` and `simple`; an image's placement and file path. `confidence`
is `exact`, `estimated` or `unavailable`, with `why` whenever it is not
`exact`. A self-intersecting outline's area is `unavailable`. An outline of
more than 2 000 vertices is not tested for self-intersection: `simple` is
`null`, never `true`.

**Regions** (`regions.json`): each closed polyline with an area, sharing
the id of its geometry record: `src` (its entity type), `area`, `area_unit`,
`area_si` (square metres, when the unit converts), `perimeter`, `centroid`,
`vertex_count`, `simple`, `confidence` and `labels` -- the ids of the texts
whose anchor lies inside it and in no smaller region.

**Block instances** (`blocks.json`): each INSERT with `block`, `at`,
`rotation_deg`, `scale`, `mirrored` and `attribs` (tag to value), and each
table (ACAD_TABLE) -- the block reference that draws it -- with the same
placement fields and `table` in place of `attribs`: `rows`, `columns` and
`cells`, every cell with text by `row` and `column` (0 at the top left),
its `text` readable, its `raw` text as written when that differs, and the
`span` of columns and rows it covers. A cell another cell's span covers says
nothing of its own and is not listed. When the drawing's reader did not read
the table's cells, `rows`, `columns` and `cells` are `null` and `why` says
so -- the contents are unknown, not empty. A table's cell texts are indexed
in `strings.json` to the table's record; the texts its block draws are also
text records, under the table's ID. The block definitions are in
`drawing.json`.

## String index

`strings.json` maps normalized strings to record ids: every text, every
dimension label, every attribute value and every table cell's text. Normalization is Unicode NFKC
(so `㎡` becomes `m2` and full-width digits become ASCII), the fraction
slash as `/`, lower-casing and whitespace collapsed to single spaces. Each
string is also indexed with all spaces removed, so `32.5 m2` and `32.5m2`
meet. A lookup goes from the string to ids, from an id to its record file
through `shard_index`, and from the record to its tiles.
