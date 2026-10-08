# Contributing

Small reviewed changes, clear support claims, and reusable test fixtures are welcome. The specification belongs in [open-part-protocol](https://github.com/Open-Part-Protocol/open-part-protocol); viewer implementation belongs here.

Before opening a pull request, run `cargo fmt --check`, `cargo clippy --all-targets --locked -- -D warnings`, and `cargo test --locked`. Include a reproducible `.opp` example for parser or geometry changes. Do not commit proprietary CAD files, personal production records, or real supplier certificates without permission.

For new preview capabilities, test the original resource's units, placement, completeness, and unsupported paths. Keep unimplemented engineering semantics visible. Retaining a failed measured result and its separate disposition is essential.

Schema snapshot updates must identify the protocol commit and update examples and the support matrix together. Generated bundles and build outputs belong in CI artifacts, not in the Git source tree.
