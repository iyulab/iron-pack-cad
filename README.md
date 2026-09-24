# iron-pack-cad

A CAD drawing as a package an LLM or a vision model can read: an overview image sized to the model's patch budget, a pyramid of overlapping tiles with JSON sidecars saying what is on each, one image per paper layout, and JSON records with the exact numbers a picture cannot give -- lengths, areas, dimension values, texts -- each pointing at the pixels it is drawn in.

Built as a tool to be handed to an agent. It contains no AI of its own: the same drawing and options give the same bytes.

Input is the [uncad-model](https://github.com/iyulab/uncad-model) entity model; the images are drawn by [iron-render-cad](https://github.com/iyulab/iron-render-cad).

## What it is not

- Not a file parser. It takes a drawing already read into the model.
- Not a judge. Where a drawing states two values that may disagree -- a dimension's stored measurement and the one its points give -- both are written as facts, with their difference; which one to trust is the reader's decision.
- Not an ML library. It performs no inference.

## Status

0.x, in development. The package layout is described in the crate documentation.

## License

MIT (see `LICENSE`). The bundled font `fonts/UncadSans-Regular.otf` is a Noto Sans KR subset under the SIL Open Font License 1.1 (see `fonts/OFL-NotoSansKR.txt` and `fonts/README.md`).
