# Safety Core Mega-Pass 18

- Typed `DecisionContext` matching scopes remembered answers by direction/backends/object scope.
- Existing kind-only request API remains available for compatibility.
- Remote Edit records a remote content baseline and refuses stale-editor overwrites.
- Recovery journal can now be drained durably one successful entry at a time.

Deferred to the next pass: wiring journal phases into every reliable transfer, native file watching/session stop lifecycle, and Inspector permission controls.
