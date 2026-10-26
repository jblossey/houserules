## Summary

<!-- What does this change do, and why? -->

## Evidence bundle

<!-- Write this section with houserules bundle write. It copies the header of each run from the capture file. -->

Purpose: <one or two sentences>
Intent: <source>:<ref>
Work items: <id>, <id>
Risk tier: <tier>
Criterion: <the criterion that selected the tier>
Budget reason: <only when the diff is above the tier's budget>

```houserules-run
<the header line of a capture file>
<the last lines of the run's output>
```

## Checklist

- [ ] Commits follow Conventional Commits (type, lowercase subject, header ≤ 100 chars, body lines ≤ 100 chars)
- [ ] No co-author, session, or tool-attribution line on any commit or in this description
- [ ] `mise run test` passes
- [ ] `mise run lint` passes (shellcheck, `cargo deny check`, `houserules check-knowledge`, `houserules check-backlog`, `houserules render --check`)
- [ ] New or changed knowledge is recorded in `knowledge/*.json` and rendered (`houserules render`)
