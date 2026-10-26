## ADDED Requirements
### Requirement: Limit for each client
The system SHALL answer a client that sends more than 100 requests in one minute with status 429.

#### Scenario: Limit exceeded
- **WHEN** a client sends its 101st request within one minute
- **THEN** the system answers with status 429

### Requirement: Retry-After header
The system SHALL send a Retry-After header with every answer of status 429.

#### Scenario: Header present
- **WHEN** the system answers with status 429
- **THEN** the answer holds a Retry-After header

## MODIFIED Requirements
### Requirement: Error format
The system SHALL write every error answer as a JSON object with the keys `code` and `message`.

#### Scenario: Error answer
- **WHEN** the system answers with an error status
- **THEN** the body is a JSON object with the keys `code` and `message`

## REMOVED Requirements
### Requirement: Burst allowance
**Reason**: The limit for each client replaces it.
**Migration**: Clients that need a higher limit ask for a new key.

## RENAMED Requirements
- FROM: `### Requirement: Throttle`
- TO: `### Requirement: Rate limit`
