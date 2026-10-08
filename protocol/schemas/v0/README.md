# OPP V0 JSON Schemas

Version: `0.1.0-draft.1`. Dialect: JSON Schema 2020-12.

- [Common definitions](common.schema.json): 80 reusable types, including typed requirements and physical-evidence records.
- [Manifest](manifest.schema.json), [design](design.schema.json), [as-built](as-built.schema.json), [bindings](bindings.schema.json), [presentation](presentation.schema.json), [conversion report](conversion-report.schema.json).

The generated [field dictionary](https://github.com/Open-Part-Protocol/open-part-protocol/blob/fa0a0e66e5d33b493b05dfc06d8fab5542d609bd/docs/spec/v0/data-dictionary.md) is a readable inventory of every serialized field.

Entry schemas reference the common schema using an exact URN, for example `urn:opp:schema:0.1.0-draft.1:common`. Register all schemas locally by `$id` before validation. Do not fetch `$ref` content from a package or the internet. Unknown core fields are rejected; explicit extensions are the forward-compatibility mechanism.

Schema constraints validate structure and basic conditional fields. Cross-collection references, graph cycles, dimensional compatibility, occurrence paths, timestamps, resource hashes, and engineering meaning require additional checks. The [common types](https://github.com/Open-Part-Protocol/open-part-protocol/blob/fa0a0e66e5d33b493b05dfc06d8fab5542d609bd/docs/spec/v0/common-types.md), [model docs](https://github.com/Open-Part-Protocol/open-part-protocol/blob/fa0a0e66e5d33b493b05dfc06d8fab5542d609bd/docs/spec/v0/README.md), and [checker scope](https://github.com/Open-Part-Protocol/open-part-protocol/blob/fa0a0e66e5d33b493b05dfc06d8fab5542d609bd/docs/spec/v0/conformance.md) specify those boundaries.

All collections in Design and AsBuilt are required, including empty collections. This simplifies readers and keeps omission distinct from empty records. IDs and engineering decimal quantities serialize as strings. Draft schemas must not be used as a claim that every proposed engineering combination has been validated by specialists.
