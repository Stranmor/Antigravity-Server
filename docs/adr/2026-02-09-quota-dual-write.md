# ADR: Quota Persistence and Dual-Write Consistency

## Problem
Quota refresh updates are not fully persisted across storage backends. Protected model state is lost in PostgreSQL, and dual-write paths can use mismatched identifiers.

## Constraints
- Keep existing storage abstractions and error handling patterns.
- No new dependencies or schema changes.

## Decision
Extend the quota update interface to optionally include protected model data and persist it to PostgreSQL. Align dual-write flows to use email to map JSON accounts to PostgreSQL identifiers.

## Alternatives Considered
1. Derive protected models from quota at read time: rejected because it loses explicit protection state.
2. Store separate mapping table for protected models: rejected due to schema change requirement.
