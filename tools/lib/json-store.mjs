// SPDX-License-Identifier: MIT
// Copyright (c) 2026 Jannis Blossey
// Shared JSON loading and JSON-Schema-subset validation for the kb and
// backlog CLIs. Node built-ins only.
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

/** Finds the repo root from `cwd` so every tool works from any directory inside the repo. */
export function repoRoot(cwd = process.cwd()) {
  return execFileSync('git', ['rev-parse', '--show-toplevel'], {
    cwd,
    encoding: 'utf8',
  }).trim();
}

/** Serializes `value` as indented JSON with a trailing newline, the CLIs' only output format. */
export const emit = (value) => `${JSON.stringify(value, null, 2)}\n`;

/** Reads and parses a JSON file, naming the file in any read or parse error. */
export function readJson(path) {
  let text;
  try {
    text = readFileSync(path, 'utf8');
  } catch (error) {
    throw new Error(`${path}: ${error.message}`, { cause: error });
  }
  try {
    return JSON.parse(text);
  } catch (error) {
    throw new Error(`${path}: invalid JSON (${error.message})`, {
      cause: error,
    });
  }
}

/** Lists the `.json` files directly inside `dir`, sorted by name. */
export function listJsonFiles(dir) {
  return readdirSync(dir)
    .filter((name) => name.endsWith('.json'))
    .toSorted()
    .map((name) => join(dir, name));
}

/** Accumulates validation error messages in order, for reporting once collection ends. */
export class Errors {
  constructor() {
    this.list = [];
  }
  add(message) {
    this.list.push(message);
  }
  get any() {
    return this.list.length > 0;
  }
}

function deref(root, ref) {
  if (!ref.startsWith('#/')) throw new Error(`unsupported $ref ${ref}`);
  return ref
    .slice(2)
    .split('/')
    .reduce((node, key) => {
      if (node == null || !(key in node))
        throw new Error(`unresolved $ref ${ref}`);
      return node[key];
    }, root);
}

function hasType(value, type) {
  const types = Array.isArray(type) ? type : [type];
  return types.some((t) => {
    if (t === 'null') return value === null;
    if (t === 'array') return Array.isArray(value);
    if (t === 'object')
      return (
        value !== null && typeof value === 'object' && !Array.isArray(value)
      );
    if (t === 'integer') return Number.isInteger(value);
    return typeof value === t;
  });
}

/**
 * Validates `value` against a JSON Schema subset: local `$ref`, `type`
 * (string or list), `enum`, `pattern`, `minLength`, `maxLength`, `minimum`,
 * `items`, `uniqueItems`, `required`, `properties`, `additionalProperties`
 * (`false` or a schema). Every violation is appended to `errors` as
 * `<at>: <problem>`.
 */
export function validate(value, schema, at, errors, root = schema) {
  if (schema.$ref) {
    validate(value, deref(root, schema.$ref), at, errors, root);
    return;
  }
  if (schema.enum && !schema.enum.includes(value)) {
    errors.add(
      `${at}: must be one of ${schema.enum.map((v) => JSON.stringify(v)).join(', ')}`,
    );
    return;
  }
  if (schema.type && !hasType(value, schema.type)) {
    errors.add(
      `${at}: must be ${Array.isArray(schema.type) ? schema.type.join(' or ') : schema.type}`,
    );
    return;
  }
  if (typeof value === 'string') {
    if (schema.pattern && !new RegExp(schema.pattern).test(value))
      errors.add(`${at}: must match ${schema.pattern}`);
    if (schema.minLength != null && value.length < schema.minLength)
      errors.add(`${at}: shorter than ${schema.minLength}`);
    if (schema.maxLength != null && value.length > schema.maxLength) {
      errors.add(`${at}: longer than ${schema.maxLength} characters`);
    }
  }
  if (
    typeof value === 'number' &&
    schema.minimum != null &&
    value < schema.minimum
  ) {
    errors.add(`${at}: below ${schema.minimum}`);
  }
  if (Array.isArray(value)) {
    if (schema.items)
      value.forEach((item, i) =>
        validate(item, schema.items, `${at}[${i}]`, errors, root),
      );
    if (
      schema.uniqueItems &&
      new Set(value.map((v) => JSON.stringify(v))).size !== value.length
    ) {
      errors.add(`${at}: items must be unique`);
    }
  }
  if (hasType(value, 'object')) {
    for (const key of schema.required ?? [])
      if (!(key in value)) errors.add(`${at}: missing "${key}"`);
    for (const [key, child] of Object.entries(value)) {
      const childSchema = schema.properties?.[key];
      if (childSchema)
        validate(child, childSchema, `${at}.${key}`, errors, root);
      else if (schema.additionalProperties === false)
        errors.add(`${at}: unknown field "${key}"`);
      else if (typeof schema.additionalProperties === 'object') {
        validate(
          child,
          schema.additionalProperties,
          `${at}.${key}`,
          errors,
          root,
        );
      }
    }
  }
}
