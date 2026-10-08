# Design and verification

The [accepted visual concept](concept.png) establishes a light engineering review workspace: a product rail on the left, real geometry in the middle, a requirement inspector on the right, and a record table below. It is a design reference, never a substitute geometry asset.

## Comparison points

| Point | Native implementation |
| --- | --- |
| Header and primary action | OPP Viewer identity, teal Open .opp action, native file dialog |
| Product identity | Definition name, part number, revision, package type, physical serial/lot selector |
| Product structure | Expanded nested occurrences with component selection and retained full paths |
| Geometry workspace | Actual STEP tessellation, light background/grid, orbit/pan/zoom, fit/reset, scan overlay |
| Requirement inspector | Nominal/allowed/measured values, separate conformance and acceptance, source bindings |
| Record area | Requirements, Production, Equipment, Evidence, Package tabs |
| Status colors | Teal selection/pass, red failure, amber deviation/warnings |
| Footer | Package check status, limits of certification, current viewer version |

The native window's visual check and screenshot are recorded after exercising the review flow. Tests and CI remain the authoritative executable verification of package behavior.

## Intentional differences from the concept

The concept uses illustrative labels. The application displays the exact package name (`Synthetic machined block`), quantities including their recorded precision, and actual source entity labels. The viewer version is `0.1.0`, not the concept's `1.0.0`. Dimension arrows and face highlighting are absent because the current renderer cannot verify their source association. The Examples menu provides working fixtures; narrow windows move the inspector to a separate window. Native egui widgets and font metrics differ from the conceptual rendering.

## Automated coverage

The integration suite covers all five protocol examples, transport corruption, independently pinned geometry, missing resource pins, false numeric passes, calibration/deviation effectivity, cross-subject results, nested/reused occurrence paths and placement, unsafe ZIP paths/symlinks, duplicate JSON keys, schema errors, scan units/finite values, real STEP dimensions, and graceful unsupported previews.

The platform workflow also checks formatting/lints, runs a headless package review, builds native releases, and packages dependency license notices. GUI checks use the actual macOS application; Windows and Linux GUI interaction will need additional native review before a formal supported release.
