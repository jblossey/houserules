# Add rate limiting to the public API

## Why
One client that retries in a tight loop takes the API down for every other client. A limit for each client keeps a noisy client from starving the rest.

## What Changes
- Add a limit for each client and answer an excess request with status 429.
- Rename the "Throttle" requirement to "Rate limit".
- Remove the "Burst allowance" requirement.

## Impact
- Affected specs: api
- Affected code: the gateway middleware
