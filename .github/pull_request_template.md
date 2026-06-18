<!--
Thanks for contributing to ocpp-rs! Keep the sections below — especially
"Real use case" — so a reader who isn't deep in the code understands *why*
this change matters, not just *what* it does.
-->

## Summary

<!-- One or two sentences: what this PR does and which OCPP use case / milestone it advances. -->

`Closes #<issue>`

## Real use case

<!--
REQUIRED. Describe a concrete, real-world scenario this change enables or fixes,
from the operator's / driver's / charge-point's point of view — not the code's.
A good "real use case" lets someone unfamiliar with the internals picture when
and why the feature is used.

Example: "A driver's RFID card is reported stolen and blocked in the back
office. Charge points cache Authorize results for offline operation, so the
revoked card could still start a charge from a stale cache. The operator sends
ClearCache so the next swipe forces a fresh Authorize round-trip, which now
returns Blocked."
-->

## What changed

<!-- The implementation, by crate/module. Note design decisions, failure modes, trust boundaries. -->

## What was ported

<!--
If this ports from the mobilityhouse/ocpp Python reference, link the source
module(s) (e.g. ocpp/v16/call.py, examples/v16/charge_point.py) so reviewers can
check fidelity.
-->

## Test plan

<!--
- cargo fmt --all --check
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --workspace
- New tests added (unit / conformance) and what they cover.
-->

## Acceptance criteria

<!-- Mirror the issue's checklist; tick what this PR satisfies. -->

## Known gaps / notes

<!-- Anything intentionally out of scope, with a follow-up issue link if applicable. -->
