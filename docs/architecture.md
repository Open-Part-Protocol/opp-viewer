# Architecture

The viewer is a native Rust executable using `eframe`/`egui` for the interface and system file dialogs through `rfd`. It needs no browser, JavaScript runtime, Python installation, Excel, or hosted backend.

## Package review

`src/package.rs` reads a ZIP-backed `.opp` package into a bounded in-memory resource map. It rejects unsafe paths, symlinks, collisions, unlisted files, unsupported compression, oversized members, and duplicate JSON keys. Original resource bytes are retained without extracting or executing their content.

The seven draft JSON schemas are embedded at compilation. A custom schema retriever resolves their fixed URNs from memory. Network and filesystem schema retrieval are disabled. Every inventoried resource is checked against its size and SHA-256; the design snapshot and resource pins are checked independently of the transport inventory.

The product graph is expanded into occurrence paths with accumulated rigid transforms in millimetres. A repeated component retains its full path, so two instances of the same definition cannot share measurements merely because they share a part number. Actual evaluations and observations are selected by physical subject ID as well as requirement ID. The latest evaluation is shown together with its own associated observation; full history remains in the source document.

Structural checks also cover selected references, serial identities, observation/run dates, referenced calibration validity, simple decimal limit comparisons, and the scope/effectivity of an accepted deviation. These are a useful subset of the draft invariants, not a complete conformance implementation. Unknown required extension semantics remain visible as warnings, and raw JSON remains available.

## Geometry

`src/geometry.rs` reads supported STEP shells through `truck-stepio`, triangulates through `truck-meshalgo`, and applies OPP occurrence transforms. The adapter checks explicit units and face conversion completeness before publishing a preview. A geometry error does not invalidate otherwise reviewable metadata.

`src/viewport.rs` projects actual tessellated triangles into an interactive orthographic view. Faces are depth sorted, shaded, and drawn with egui's native renderer. Edge strokes are drawn before filled faces to avoid displaying hidden edges through the solid. This initial renderer has no dedicated 3D depth buffer or full hidden-line calculation. It is intended for limited review geometry; a dedicated rendering pipeline is a future improvement.

Registered ASCII PLY points are converted from their stated unit and transformed through the selected subject's full occurrence path. Unregistered scans, unsupported coordinate frame chains, and unsupported data encodings remain inspectable as evidence but have no overlay.

Source STEP entity references and rendered triangles do not yet have a verified one-to-one association. The inspector explicitly disables face highlighting rather than implying an unproven association.

## Interface and loading

`src/app.rs` displays product structure, the geometry preview, a requirement inspector, and Requirements/Production/Equipment/Evidence/Package tabs. Smaller windows move the inspector into a separate window. The serial/lot selector scopes actual results and scan overlays to a single physical subject.

Package loading and tessellation for files opened in the interface run on a background thread, communicating through a channel. The current package stays visible while a replacement loads; a failed open preserves it. The initial command-line file is checked before launching the window.

Binary evidence can be saved to an explicitly selected destination. Text previews and JSON views are inert. Archive filenames never become extraction destinations, and package content does not run programs or trigger network requests.

## Portability and builds

Cargo pins the Rust toolchain and dependency graph. CI runs the same integration tests and a headless inspection on Linux, Windows, and macOS, then builds native binaries. macOS includes ARM64 and Intel targets. `tools/build_dist.py` packages these outputs and creates the macOS `.app` structure; it is a development packaging utility, not an app runtime dependency.

The protocol snapshot is deliberately separate from the viewer. Updating it requires a reviewed commit changing schemas, examples, provenance, support claims, and tests together.
