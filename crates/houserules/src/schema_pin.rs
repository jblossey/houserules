//! The schema-pin build-test mechanism (docs/specs/2026-09-04-batch-15-
//! tier2-spec.md §3: "a build test that pins the serde models against the
//! vendored schema files"). Shared by every model layer's own `#[cfg(test)]`
//! tests (`backlog::model`, `rules::check_shape`) so the mechanism is
//! written once and each schema-pin test only supplies the model type, the
//! schema pointer, and one hand-written valid sample. `rules::deliverables`
//! (batch 17 T1's own such caller) was deleted at T3, once none of that
//! task's three surfaces turned out to need a typed deliverables model
//! (`rules/mod.rs`'s own module doc has the full account) -- this module's
//! mechanism itself is unaffected, and `check_shape`'s pin tests still
//! exercise it fully.
//!
//! `#[cfg(test)]`-only (declared that way in `main.rs`): this is test
//! infrastructure, not a shipped part of the binary.
//!
//! ## What "pinned" means here, and its limits
//!
//! `assert_object_pinned::<T>(schema, pointer, sample)` proves, for one
//! JSON-Schema object definition at `pointer` (e.g. `/$defs/item`) in
//! `schema`:
//! - **field presence**: `sample`'s own keys, the model `T`'s deserialized-
//!   then-reserialized keys, and the schema's declared `properties` keys
//!   are all the same set, in both directions -- neither side can have a
//!   field the other lacks.
//! - **required sets**: for every property, removing its key from `sample`
//!   and asking the *schema itself* (via `rules::validate`, the same
//!   generic engine `checkBase` already runs -- reusing it rather than
//!   re-deriving "required" from the schema JSON keeps this one write
//!   path, `quality.principles`) whether the result is still valid must
//!   agree with whether `T` still deserializes. A field the schema
//!   requires but allows `null` for (`RequiredNullable<_>`, not `Option<_>`)
//!   is exactly the case this catches: a plain `Option<_>` would silently
//!   accept the key missing (serde's own implicit default for `Option`
//!   fields), which the schema does not.
//! - **the null state**: for every property whose schema type or enum
//!   admits `null` (whether or not the property is required), setting it
//!   to `null` in a copy of `sample` must (a) still validate against the
//!   schema, (b) still deserialize into `T`, and (c) round-trip back out
//!   as a *present* `null` -- not vanish, and not silently become the
//!   missing-key state. Fix round 1 (review issue 1, batch 17 T1) added
//!   this check after finding the state it closes: `sample` carries
//!   exactly one value per property, so an *optional*, nullable property
//!   modelled plain `Option<T>` (the now-deleted `rules::deliverables`'
//!   `auditRow.level`, T1's own worked example) passed every other check
//!   here while silently dropping `"level": null` on every judged row in
//!   the committed fixtures -- the fix (`RequiredNullable<T>` for a
//!   required-nullable property, `Option<Option<T>>` plus
//!   `deserialize_optional_nullable` for an optional-nullable one) lived in
//!   `crate::json_shape`, deleted alongside `rules::deliverables` at T3
//!   once neither had a remaining consumer (`rules/mod.rs`'s module doc).
//!   This check itself is unaffected by that deletion -- it runs for any
//!   future null-admitting property either of this mechanism's two current
//!   callers (`backlog::model`, `check_shape`) pins; neither happens to
//!   declare one today.
//! - **round-trip fidelity**: the *entire* re-serialized model, not only
//!   its top-level key set, is compared against `sample` for exact
//!   equality. This is what stops a dropped or altered *nested* field from
//!   passing: the top-level key-set check above cannot see inside a
//!   property's own value, only that the property's key exists.
//! - **validity**: `sample` itself passes `rules::validate` against the
//!   definition -- the hand-written fixture is a genuinely schema-valid
//!   instance, not merely key-shaped.
//!
//! `assert_enum_pinned::<T>(schema, pointer, variants)` proves the same two
//! directions for a JSON-Schema `enum`: every string the schema lists has a
//! `variants` entry that serializes to it and deserializes back losslessly,
//! and `variants` names nothing the schema does not. Enumerating `variants`
//! by hand is the mechanism's one honest limit: Rust has no runtime
//! reflection over an enum's variants, so nothing stops a future variant
//! added to the type from being left out of a call site's `variants` list
//! -- the schema side of the pin would then go unchecked for that variant
//! until the call site is updated by hand.
//!
//! ## What still goes unchecked
//!
//! - `pattern`, `minLength`, `maxLength`, and `minimum`: the model layer
//!   represents every constrained string as a plain `String` and every
//!   constrained number as a plain integer/float, leaving those
//!   constraints to the schema validator alone (the same `rules::validate`
//!   this module's oracle calls) rather than duplicating a regex or range
//!   engine in the type system -- `quality.principles`' prefer-a-library
//!   rule, applied to "the schema itself is the library".
//! - A schema `enum` whose members are not all strings (the now-deleted
//!   `rules::deliverables`'s `auditSummary.empty_range`,
//!   `{"enum": [true]}`, was this mechanism's one worked example) is
//!   outside `assert_enum_pinned`, which reads a schema enum through
//!   `Value::as_str` and so sees only string-valued members -- such a
//!   property needs its own dedicated type with hand-written
//!   `Serialize`/`Deserialize` and its own direct unit tests instead of
//!   this shared mechanism, the way `rules::deliverables::EmptyRangeTrue`
//!   (T1, deleted at T3 alongside the rest of that file) once did.
//! - A model type that declares no `assert_object_pinned`/
//!   `assert_enum_pinned` call site anywhere is not pinned at all, and
//!   nothing enforces that every type gets one -- there is no derive or
//!   lint here, only the discipline of adding a call site for every new
//!   type (batch 17 T1 fix round 1's `Finding`, missing through the first
//!   cut, was the review's cited instance).

use std::collections::BTreeSet;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

/// `true` when a property's own schema fragment (resolving one `$ref`
/// indirection if present) admits the JSON value `null` -- its `type`
/// includes `"null"`, or its `enum` lists the JSON literal `null` among
/// other values. Drives `assert_object_pinned`'s null-state check: a
/// property this returns `false` for is never mutated to `null`, because
/// the schema itself would reject that mutation (a `sample_errors`
/// assertion inside this module would fail on a fixture that did), not
/// because the model's own handling goes unchecked.
fn admits_null(schema_root: &Value, property_schema: &Value) -> bool {
    if let Some(reference) = property_schema.get("$ref").and_then(Value::as_str)
        && let Some(pointer) = reference.strip_prefix('#')
        && let Some(target) = schema_root.pointer(pointer)
    {
        return admits_null(schema_root, target);
    }
    let type_admits_null = match property_schema.get("type") {
        Some(Value::String(t)) => t == "null",
        Some(Value::Array(types)) => types.iter().any(|t| t.as_str() == Some("null")),
        _ => false,
    };
    if type_admits_null {
        return true;
    }
    property_schema
        .get("enum")
        .and_then(Value::as_array)
        .is_some_and(|values| values.iter().any(Value::is_null))
}

/// Proves `T`'s serde surface matches the object definition at `pointer`
/// (a JSON Pointer such as `/$defs/item`) in `schema`, using `sample` (a
/// hand-written, schema-valid JSON object) as the fixture. See the module
/// doc for exactly what "matches" checks and does not.
pub(crate) fn assert_object_pinned<T>(schema: &Value, pointer: &str, sample: &Value)
where
    T: DeserializeOwned + Serialize,
{
    let def = schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{pointer}: not found in the schema"));
    let properties: BTreeSet<String> = def
        .get("properties")
        .and_then(Value::as_object)
        .map(|map| map.keys().cloned().collect())
        .unwrap_or_default();

    let sample_object = sample
        .as_object()
        .unwrap_or_else(|| panic!("{pointer}: the sample must be a JSON object"));
    let sample_keys: BTreeSet<String> = sample_object.keys().cloned().collect();
    assert_eq!(
        sample_keys, properties,
        "{pointer}: the sample's keys must match the schema's declared properties exactly"
    );

    let mut sample_errors = Vec::new();
    crate::rules::validate(sample, def, "$", &mut sample_errors, schema);
    assert!(
        sample_errors.is_empty(),
        "{pointer}: the sample is not itself schema-valid: {sample_errors:?}"
    );

    let model: T = serde_json::from_value(sample.clone()).unwrap_or_else(|error| {
        panic!("{pointer}: the sample failed to deserialize into the model: {error}")
    });
    let round_tripped = serde_json::to_value(&model)
        .unwrap_or_else(|error| panic!("{pointer}: the model failed to serialize back: {error}"));
    let round_tripped_keys: BTreeSet<String> = round_tripped
        .as_object()
        .unwrap_or_else(|| panic!("{pointer}: the model must serialize to a JSON object"))
        .keys()
        .cloned()
        .collect();
    assert_eq!(
        round_tripped_keys, properties,
        "{pointer}: the model's serialized field set must match the schema's properties exactly"
    );
    assert_eq!(
        &round_tripped, sample,
        "{pointer}: the model's full round trip must reproduce the sample exactly, not only its \
         top-level keys -- a dropped or altered nested field is otherwise invisible to this check"
    );

    let mut with_extra = sample_object.clone();
    with_extra.insert(
        "__schema_pin_unknown_field_probe__".to_string(),
        Value::Bool(true),
    );
    let with_extra_value = Value::Object(with_extra);
    let mut extra_errors = Vec::new();
    crate::rules::validate(&with_extra_value, def, "$", &mut extra_errors, schema);
    let schema_rejects_extra = !extra_errors.is_empty();
    let model_rejects_extra = serde_json::from_value::<T>(with_extra_value).is_err();
    assert_eq!(
        model_rejects_extra, schema_rejects_extra,
        "{pointer}: the model's tolerance for an unrecognized field (rejects it: \
         {model_rejects_extra}) must match the schema's (rejects it: {schema_rejects_extra}) -- \
         add #[serde(deny_unknown_fields)] if the schema sets additionalProperties: false here"
    );

    for key in &properties {
        let mut without = sample_object.clone();
        without.remove(key);
        let without_value = Value::Object(without);

        let mut errors = Vec::new();
        crate::rules::validate(&without_value, def, "$", &mut errors, schema);
        let schema_allows_missing = errors.is_empty();
        let model_allows_missing = serde_json::from_value::<T>(without_value).is_ok();
        assert_eq!(
            model_allows_missing, schema_allows_missing,
            "{pointer}.{key}: the model's presence requirement (missing key still parses: \
             {model_allows_missing}) must match the schema's (missing key still validates: \
             {schema_allows_missing})"
        );

        let property_schema = def
            .pointer(&format!("/properties/{key}"))
            .cloned()
            .unwrap_or(Value::Null);
        if !admits_null(schema, &property_schema) {
            continue;
        }
        let mut with_null = sample_object.clone();
        with_null.insert(key.clone(), Value::Null);
        let with_null_value = Value::Object(with_null);

        let mut null_errors = Vec::new();
        crate::rules::validate(&with_null_value, def, "$", &mut null_errors, schema);
        assert!(
            null_errors.is_empty(),
            "{pointer}.{key}: the schema admits null for this property but rejects it: {null_errors:?}"
        );
        let null_model: T = serde_json::from_value(with_null_value).unwrap_or_else(|error| {
            panic!("{pointer}.{key}: a present null failed to deserialize into the model: {error}")
        });
        let null_round_tripped = serde_json::to_value(&null_model).unwrap_or_else(|error| {
            panic!("{pointer}.{key}: the model failed to serialize a present null back: {error}")
        });
        assert_eq!(
            null_round_tripped.get(key.as_str()),
            Some(&Value::Null),
            "{pointer}.{key}: a present null must round-trip as a present null, not vanish or \
             change -- {null_round_tripped:?}"
        );
    }
}

/// Proves `T`'s serde surface matches the `enum` at `pointer` in `schema`:
/// every schema value round-trips through some entry of `variants`, and
/// `variants` names nothing the schema does not. See the module doc for
/// this mechanism's one honest limit (`variants` is hand-enumerated).
pub(crate) fn assert_enum_pinned<T>(schema: &Value, pointer: &str, variants: &[T])
where
    T: DeserializeOwned + Serialize,
{
    let def = schema
        .pointer(pointer)
        .unwrap_or_else(|| panic!("{pointer}: not found in the schema"));
    let schema_values: BTreeSet<String> = def
        .get("enum")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{pointer}: has no \"enum\""))
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();

    let model_values: BTreeSet<String> = variants
        .iter()
        .map(|variant| {
            serde_json::to_value(variant)
                .ok()
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_else(|| {
                    panic!("{pointer}: a variant did not serialize to a JSON string")
                })
        })
        .collect();
    assert_eq!(
        model_values, schema_values,
        "{pointer}: the model's enum variants must match the schema's declared enum exactly"
    );

    for value in &schema_values {
        let parsed: T =
            serde_json::from_value(Value::String(value.clone())).unwrap_or_else(|error| {
                panic!("{pointer}: {value:?} did not deserialize into the model: {error}")
            });
        let round_tripped = serde_json::to_value(&parsed).expect("serialize enum variant");
        assert_eq!(
            round_tripped.as_str(),
            Some(value.as_str()),
            "{pointer}: {value:?} did not round-trip through the model"
        );
    }
}
