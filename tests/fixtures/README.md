# Test fixtures

Hand-authored R2000 (`$ACADVER AC1015`) ASCII DXF files, written from scratch by
this project -- no third-party drawing was copied -- and distributed under this
repository's MIT licence. The tests read them with `undxf` (a dev-dependency) and
pass the header variables the file states as a `Header`; what each test expects is
stated in the test itself, worked out from the file's group codes.

They keep their bytes (`.gitattributes`: `-text`): the CP949 fixture is not UTF-8
by design, and every file has CRLF line endings.

| File | Purpose |
|---|---|
| `cp949_r2000.dxf` | CP949 (`ANSI_949`) strings in TEXT, MTEXT and a LAYER name |
| `dimlfac12_r2000.dxf` | `$DIMLFAC 12.0` (header and STANDARD style) with a rotated DIMENSION whose `act_measurement` is 10.0, its `*D1` block bound |
| `hatched_viewport_r2000.dxf` | The twisted-viewport fixture plus a pattern HATCH in model space (under the viewport) and one in paper space (outside its frame) |
| `hidden_layers_r2000.dxf` | One LINE per layer state (on, off, frozen, non-plotting, `Defpoints`, locked), an invisible LINE and a 0.50 mm DASHED one |
| `nested_attrib_r2000.dxf` | A block with an ATTDEF inserted inside another block, its ATTRIB value owned by the block record: the attribute of a *nested* block reference |
| `plot_origin_r2000.dxf` | A LAYOUT `Layout1` in inches (ANSI B landscape) with asymmetric margins and a non-zero plot origin (DXF 46/47); a paper-space border LWPOLYLINE and a 1:5 plan VIEWPORT over a model LINE |
| `title_block_r2000.dxf` | A drawing whose every string is in *paper* space: a title TEXT and a title-block INSERT on an A4 sheet over a model with no text at all |
| `twisted_viewport_r2000.dxf` | A paper-space VIEWPORT with `VIEWTWIST` 30 degrees and every view field, plus a LAYOUT `Layout1` (A4 landscape) bound to `*Paper_Space` and to the VIEWPORT |
| `viewport_states_r2000.dxf` | One VIEWPORT per state a sheet distinguishes (on; on a frozen layer; off; non-plan) plus two page setups |
