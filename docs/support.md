# Prototype support matrix

Target draft: `0.1.0-draft.1`. “Review” means displaying the recorded definition and results; it does not mean certifying every engineering rule in that profile.

| Area | Initial behavior | Limits |
| --- | --- | --- |
| Packages | ZIP inventory, exact resource bytes, hashes, embedded schemas | 64 MiB compressed, 64 MiB per member, 256 MiB total expanded, 2,048 entries, compression ratio at most 200 |
| JSON | Strict duplicate-key rejection and draft schemas | 8 MiB per JSON resource; text previews stop at 64,000 characters |
| Design snapshot | Design document hash and pinned geometry/bindings/supporting resources | Authenticity/signatures not assessed |
| Parts/assemblies | Definition tree, nested instances, rigid placements, full paths | At most 2,000 expanded nodes and depth 64; reflected/scaled transforms rejected |
| Actual identity | Selected subject/serial/lot, scoped measurements and scans | No aggregate lot certification; one subject selected at a time; latest evaluation and its associated observation shown, older results preserved in Package |
| Requirements | Typed definitions, bounds, results, acceptance, raw details | Complex GD&T/material/process semantics are displayed, not independently interpreted |
| Evaluations | Selected relationship/time checks; decimal simple limits and scoped deviations | No uncertainty-based decision rules, complete FAIR coverage engine, or AS9102 compliance certificate |
| Production | Events, material lots, operators/company/cell references in full records | Recorded history only; no production execution controls |
| Equipment | Equipment/calibrations/run views; calibration date checks for observations | No claim of real metrological traceability or physical calibration verification |
| STEP geometry | Experimental supported B-rep tessellation, explicit SI length unit, OPP placement | STEP at most 16 MiB; at most 50,000 triangles; first nominal state; no AP242 semantic PMI conversion, conversion-based length units, mixed unit contexts, or STEP-internal assembly mappings |
| Geometry frames | Product-local geometry, occurrence transform chain | Nested representation coordinate-frame transforms not rendered |
| Topology bindings | Source entity labels and full binding documents | Individual face highlighting unavailable |
| Scans | Registered ASCII PLY point cloud with explicit unit, selected subject | At most 16 MiB and 100,000 points; no binary PLY, E57, LAS, registration solving, or deviation map |
| Evidence | List, inert text preview, original save-copy | Specialist binary formats require their own applications |
| Extensions | Preserved in raw JSON; required unknown semantics warned | No executable plugins or schema downloads |
| Desktop | Native Linux X11/Wayland, Windows, macOS window | Graphical desktop/OpenGL required; unsigned development bundles |

These limits belong to this implementation, not to the protocol itself. Exceeding a package limit produces an error; unsupported geometry produces a visible warning while metadata stays reviewable. Every screen uses data read from the package; no substitute model is rendered when geometry conversion fails.

## Next implementation work

1. Validate a broader real CAD export corpus against the pure Rust STEP adapter.
2. Provide verified render mappings for STEP entities and characteristic highlighting.
3. Add representation/state selection, nested frame support, and more scan encodings.
4. Expand profile-specific checks with explicit conformance tests and published coverage.
5. Add a dedicated 3D rendering pipeline, signed installers, file associations, and release packaging.
