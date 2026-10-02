# Changelog

Notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versioning follows
[Semantic Versioning](https://semver.org/). While the version is 0.x, a breaking change
bumps the minor version.

## [Unreleased]

## [0.3.0] - 2026-10-02

### Changed

- **Breaking:** built on `iron-render-cad` 0.4.0, whose `Fonts` and `Crop` this crate's API
  hands through (`ExportOptions::fonts`, `fonts::bundled`, `frame::renderer_crop`). Upgrade
  them together. The package is drawn on the renderer's light page, so its files are unchanged.

## [0.2.0] - 2026-10-02

### Changed

- **Breaking:** the package's units are the ones the drawing's model states
  (`CadDatabase::header`, from `uncad-model`), with or without a `Header` handed in.
  `Header::insunits` and `Header::units` are gone; an `insunits` key in a header's JSON is
  kept in `Header::other` and written back to `drawing.json` unchanged, as any other variable.
- Built on `uncad-model` 0.3.0 and `iron-render-cad` 0.3.0: a multileader's lines are drawn
  on to their leader roots in the overview and tiles.

## [0.1.0] - 2026-09-29

### Added

- Initial release. `export_package` writes an
  [uncad-model](https://github.com/iyulab/uncad-model) drawing as a directory an LLM or a
  vision model can read, its images drawn by [iron-render-cad](https://github.com/iyulab/iron-render-cad).
- The package holds an overview image sized to a patch budget, a pyramid of overlapping tiles
  with JSON sidecars, one image per paper layout, and records of texts, dimensions, geometry,
  regions and block instances with the exact numbers a picture cannot give, each pointing at its pixels.
- A string index maps normalized strings (Unicode NFKC, case and spacing folded) to records.
  The same drawing and options give the same bytes; the format is in `docs/PACKAGE.md`.
