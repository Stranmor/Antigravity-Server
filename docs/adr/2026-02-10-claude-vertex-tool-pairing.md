# ADR: Preserve Claude tool pairing after signature stripping

## Problem
Claude requests routed through Vertex AI can end up with orphaned tool_use parts after
signature stripping, which breaks role alternation and causes request rejection.

## Constraints
- Must not change tool construction or message building stages.
- Must not drop model messages after stripping.
- Must keep behavior deterministic with minimal mutation.

## Decision
Insert a placeholder text part when a model message becomes empty after stripping.
Add post-strip validation that logs tool pairing violations without mutating data.

## Alternatives considered
- Drop empty messages: rejected because it breaks role alternation.
- Rebuild message history: rejected as too invasive for this fix.
