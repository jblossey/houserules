//! Stamps, compares, and reports drift against the kit's ownership
//! baselines: for each `KIT_OWNED` file and each kit-shipped knowledge
//! entry, the SHA-256 of the content the kit itself last wrote.
//!
//! `install::update` keeps every such item in sync with the running kit's
//! payload while never destroying an adopter's own change: the baseline
//! recorded for an item is what lets `update` tell "the kit wrote this,
//! and nothing has touched it since" (safe to replace) apart from "the
//! adopter changed this" (never replace) or "the adopter never had this
//! at all yet" (safe to write for the first time). [`classify`] is the one
//! decision point every item -- file or entry alike -- passes through.

use sha2::{Digest, Sha256};

/// The lowercase hex SHA-256 digest of `content`, stable across runs and
/// platforms: the one canonical form every baseline value and comparison
/// in this module uses.
pub(crate) fn hash(content: &[u8]) -> String {
    Sha256::digest(content)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// How one owned item -- a `KIT_OWNED` file's bytes, or one knowledge
/// entry's canonical JSON -- compares to its recorded baseline and the
/// kit's current payload. [`classify`] is the only way to produce one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    /// No adopter drift: the caller writes the payload's current content
    /// and restamps the baseline to its hash. Covers four shapes alike --
    /// content already at the recorded baseline; content that already
    /// equals the CURRENT payload even though the recorded baseline is
    /// stale (both copies edited to the same ruled wording, the dogfood
    /// norm, without an intervening restamp -- nothing to keep against a
    /// baseline the kit itself has already moved past); content that
    /// happens to already equal the payload with no baseline recorded yet
    /// (a pre-baseline install's first stamp); and an item the adopter has
    /// never had at all (nothing recorded, nothing on disk).
    AtBaseline,
    /// The current content diverges from its recorded baseline (or, with
    /// none recorded, from the payload): an adopter change. The caller
    /// writes nothing and reports the item once.
    Modified,
    /// A baseline was recorded, but the item is gone: the adopter deleted
    /// it outright. The caller writes nothing and reports the item once.
    Deleted,
    /// The adopter declared ownership of this path or id in `overrides`.
    /// The caller writes nothing and reports nothing.
    Overridden,
}

/// Classifies one owned item against its recorded `baseline` hash (`None`
/// when the item has never been stamped), its `current` on-disk content
/// (`None` when the item is entirely absent), and the kit's `payload`
/// content for it. `overridden` short-circuits every other input: an
/// adopter-declared override is always [`Status::Overridden`], regardless
/// of what the content or baseline says.
///
/// `current` matching `payload` is [`Status::AtBaseline`] regardless of
/// what `baseline` records: a stale recorded baseline never reports drift
/// against content that already matches what the kit currently ships,
/// since there is nothing left for the adopter to have diverged from.
pub(crate) fn classify(
    baseline: Option<&str>,
    current: Option<&[u8]>,
    payload: &[u8],
    overridden: bool,
) -> Status {
    if overridden {
        return Status::Overridden;
    }
    let Some(current) = current else {
        return if baseline.is_some() {
            Status::Deleted
        } else {
            Status::AtBaseline
        };
    };
    let current_hash = hash(current);
    let at_recorded_baseline = baseline.is_some_and(|baseline| current_hash == baseline);
    let at_current_payload = current_hash == hash(payload);
    if at_recorded_baseline || at_current_payload {
        Status::AtBaseline
    } else {
        Status::Modified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_a_stable_lowercase_hex_sha256_digest() {
        // The standard SHA-256 test vectors for the empty message and "abc".
        assert_eq!(
            hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hash(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_ne!(hash(b"abc"), hash(b"abd"));
    }

    #[test]
    fn classify_at_baseline_when_current_matches_the_recorded_baseline() {
        let baseline = hash(b"kit content");
        assert_eq!(
            classify(
                Some(&baseline),
                Some(b"kit content"),
                b"new kit content",
                false
            ),
            Status::AtBaseline
        );
    }

    /// A stale recorded baseline (content the kit shipped BEFORE a ruled
    /// wording change) must not shadow `current` already matching what the
    /// kit ships NOW -- both copies edited together, the dogfood norm,
    /// land exactly here. There is nothing to keep: the adopter holds
    /// precisely the running payload, so this restamps silently rather
    /// than reporting drift against a baseline the kit itself has already
    /// moved past.
    #[test]
    fn classify_at_baseline_when_current_already_matches_the_payload_despite_a_stale_recorded_baseline()
     {
        let stale_baseline = hash(b"old kit content");
        assert_eq!(
            classify(
                Some(&stale_baseline),
                Some(b"new kit content"),
                b"new kit content",
                false
            ),
            Status::AtBaseline
        );
    }

    #[test]
    fn classify_modified_when_current_diverges_from_the_recorded_baseline() {
        let baseline = hash(b"kit content");
        assert_eq!(
            classify(
                Some(&baseline),
                Some(b"adopter edit"),
                b"new kit content",
                false
            ),
            Status::Modified
        );
    }

    #[test]
    fn classify_deleted_when_a_baseline_was_recorded_but_nothing_is_on_disk() {
        let baseline = hash(b"kit content");
        assert_eq!(
            classify(Some(&baseline), None, b"new kit content", false),
            Status::Deleted
        );
    }

    #[test]
    fn classify_at_baseline_with_no_recorded_baseline_when_current_already_matches_payload() {
        assert_eq!(
            classify(None, Some(b"payload content"), b"payload content", false),
            Status::AtBaseline
        );
    }

    #[test]
    fn classify_modified_with_no_recorded_baseline_when_current_diverges_from_payload() {
        assert_eq!(
            classify(
                None,
                Some(b"pre-existing content"),
                b"payload content",
                false
            ),
            Status::Modified
        );
    }

    #[test]
    fn classify_at_baseline_when_neither_a_baseline_nor_any_content_exists_yet() {
        // Nothing recorded, nothing on disk: the item is new to this
        // install and is safe to write for the first time.
        assert_eq!(
            classify(None, None, b"payload content", false),
            Status::AtBaseline
        );
    }

    #[test]
    fn classify_is_overridden_regardless_of_baseline_or_content() {
        let baseline = hash(b"kit content");
        assert_eq!(
            classify(
                Some(&baseline),
                Some(b"adopter edit"),
                b"new kit content",
                true
            ),
            Status::Overridden
        );
        assert_eq!(
            classify(Some(&baseline), None, b"new kit content", true),
            Status::Overridden
        );
        assert_eq!(
            classify(None, None, b"payload content", true),
            Status::Overridden
        );
    }
}
