## Summary

<!-- What does this change do, and why? -->

## Checklist

- [ ] Commits follow Conventional Commits (type, lowercase subject, header ≤ 100 chars, body lines ≤ 100 chars)
- [ ] No `Co-Authored-By:` or `Claude-Session:` trailer on any commit
- [ ] `mise run test` passes
- [ ] `mise run lint` passes (`tools/kb.sh check`, `tools/backlog.sh check`, shellcheck)
- [ ] New or changed knowledge is recorded in `knowledge/*.json` and rendered (`tools/kb.sh render`)
