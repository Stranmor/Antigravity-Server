# ADR: Image MIME auto-detection from base64 bytes

## Problem
Clients may send incorrect declared image MIME types (e.g., JPEG data labeled as PNG), which causes upstream rejection.

## Constraints
- Must not decode full images; only inspect a small base64 prefix.
- No new dependencies.
- Fallback behavior must preserve declared type when detection fails.

## Decision
Add a shared utility that decodes the first 16 bytes from base64 and detects common image formats by magic bytes, overriding the declared MIME type when a mismatch is found.

## Alternatives Considered
1. Trust declared MIME type and reject mismatches early.
   - Rejected: continues to fail valid payloads with incorrect client metadata.
2. Decode entire image and run full type sniffing.
   - Rejected: unnecessary overhead and violates performance constraints.
3. Add an external MIME detection crate.
   - Rejected: new dependency not allowed.
