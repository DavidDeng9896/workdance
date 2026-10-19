# Vendored OpenLess beta

This directory is the OpenLess desktop application from
<https://github.com/Open-Less/openless> branch `beta`, copied so WorkDance
ships as one host rather than a sidecar.

- Upstream commit: see `UPSTREAM_COMMIT`
- License: GNU AGPL-3.0-only (`UPSTREAM_LICENSE`)
- Copyright: OpenLess contributors

WorkDance changes in this tree:

- `src-tauri/src/workdance_host.rs` — camera, MediaPipe Hands, gate, pointer
- `src-tauri/src/commands/workdance.rs` — debug commands
- `src-tauri/src/coordinator.rs` — `dispatch_workdance_dictation_edge`
- `src-tauri/src/insertion.rs` — shared IO lock with pointer injection
- `src/components/Capsule.tsx` — four gesture states on the capsule

Do not import this tree as a second process. The gesture crate lives at
`crates/workdance-gesture` and is not part of `openless-core`.
