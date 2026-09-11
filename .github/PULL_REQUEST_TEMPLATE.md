## Summary

<!-- What does this change do, and why? -->

## Checklist

- [ ] Commits follow Conventional Commits (type, lowercase subject, header ≤ 100 chars, body lines ≤ 100 chars)
- [ ] No co-author, session, or tool-attribution line on any commit or in this description
- [ ] `mise run test` passes
- [ ] `mise run lint` passes (shellcheck, `cargo deny check`, `houserules check-knowledge`, `houserules check-backlog`, `houserules render --check`)
- [ ] New or changed knowledge is recorded in `knowledge/*.json` and rendered (`houserules render`)
