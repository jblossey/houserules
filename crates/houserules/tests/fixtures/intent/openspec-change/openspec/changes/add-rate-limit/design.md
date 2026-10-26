## Context
The gateway has no per-client state today.

## Decisions
- Count requests in a fixed window of one minute. A sliding window costs more memory for no gain at this scale.
- Key the count by the API key, not by the address of the client.
