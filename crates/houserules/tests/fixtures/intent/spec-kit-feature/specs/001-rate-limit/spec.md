# Feature Specification: Rate limiting

**Feature Branch**: `001-rate-limit`
**Created**: 2026-10-24
**Status**: Draft
**Input**: User description: "Limit the requests of each API client so that one noisy client cannot starve the rest."

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A client over its limit is told to wait (Priority: P1)

A client that sends too many requests gets a clear answer and a time to retry.

**Why this priority**: Without it, one client can take the API down for all others.

**Acceptance Scenarios**:

1. **Given** a client that sent 100 requests in one minute, **When** it sends one more, **Then** the system answers with status 429.

### Edge Cases

- A client that changes its API key starts a new count.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST count the requests of each client in a window of one minute.
- **FR-002**: The system MUST answer a client over its limit with status 429.
- **FR-003**: The system MUST send a Retry-After header with every answer of status 429.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: No client takes more than its limit of the capacity in a load test.
