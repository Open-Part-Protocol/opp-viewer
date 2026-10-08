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

The [actual native as-built window](native-preview.png) and [nested assembly with a registered scan](native-assembly.png) were captured from the macOS app after exercising the review flow. The concept and native screenshot were compared across the eight points above: overall layout, data hierarchy, actions, panels, and status colors match; the deliberate differences are listed below. Tests and CI verify package behavior separately.

## Intentional differences from the concept

The concept uses illustrative labels. The application displays the exact package name (`Synthetic machined block`), quantities including their recorded precision, and actual source entity labels. The viewer version is `0.1.0`, not the concept's `1.0.0`. Dimension arrows and face highlighting are absent because the current renderer cannot verify their source association. The Examples menu provides working fixtures; narrow windows move the inspector to a separate window. Native egui widgets and font metrics differ from the conceptual rendering.

## Native review

The macOS application was opened through its `.app` bundle. The review checked design-only records without invented actual results, the part as-built fixture, nested component selection, distinct left/right serials and width measurements, a scan overlay limited to the selected occurrence, lot identity, production detail windows, equipment/calibration views, evidence text, native file dialogs, zoom and fit. Opening a tampered package displayed its failed SHA-256 check and preserved the previous document. Orbit and pan input handling are also checked with simulated egui pointer events.

The recorded assembly has a left width of `20.002 mm` and a right width of `20.060 mm`; the latter remains failed and accepted under its deviation. This makes component-scope leakage visible during manual review.

## Automated coverage

The 22 integration tests cover all five protocol examples, transport corruption, independently pinned geometry, missing resource pins, false numeric passes, calibration/deviation effectivity, cross-subject results, nested/reused occurrence paths and placement, unsafe ZIP paths/symlinks, duplicate JSON keys, schema errors, scan units/finite values, real STEP dimensions, and graceful unsupported previews.

A separate UI input test covers orbit/pan gestures and fit/reset behavior. The platform workflow also checks formatting/lints, runs a headless package review, builds native releases, and packages dependency license notices. GUI checks use the actual macOS application; Windows and Linux GUI interaction will need additional native review before a formal supported release.
