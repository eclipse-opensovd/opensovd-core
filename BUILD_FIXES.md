# Build Fixes for opensovd-core-inc-zf

This document tracks all the fixes that were needed to get the SOVD server building and running properly.

## References

- [ISO/IEC 17978-3 - Service-Oriented Vehicle Diagnostics (SOVD) standard](https://standards.iso.org/iso/17978/-3/ed-1/en/)

## The Journey

We had over 42 compilation errors across OpenAPI schemas and Rust code. Here's what we fixed:

## OpenAPI Schema Issues

### Trailing Whitespace in YAML References
**Error:** Trailing spaces in YAML file references were breaking the code generator. The extra whitespace at the end of `$ref` lines caused the parser to fail when processing the schema references:
- `sovd-interfaces/scripts/scripts.yaml` line 191: `ExecutionId` reference had 6 trailing spaces
- `sovd-interfaces/cyclic-subscriptions/types.yaml` line 96: `ProtocolType` reference had 2 trailing spaces

**Correction:** Removed all trailing whitespace from the affected `$ref` lines, allowing the code generator to properly parse and process the schema references.

Fixed in:
- `sovd-interfaces/scripts/scripts.yaml` line 191
- `sovd-interfaces/cyclic-subscriptions/types.yaml` line 96

### External Schema Reference Problem
**Error:** The capability description was trying to reference an external OpenAPI schema from GitHub (`https://raw.githubusercontent.com/OAI/OpenAPI-Specification/refs/tags/3.1.1/schemas/v3.1/schema.yaml`), but it had a complex `webhooks` structure that crashed the Rust generator.

**Correction:** Replaced the external reference with a simple inline schema definition:
```yaml
type: object
description: An OpenAPI 3.1 specification document
additionalProperties: true
```

This prevents the generator from trying to parse the complex external schema.

Fixed in: `sovd-interfaces/capability-description/capability-description.yaml` line 73

### Discriminator Type Conflict
**Error:** The `TriggerCondition` schema had both `type: object` and `oneOf` with a discriminator pattern, which created a conflicting constraint - the `type` declaration contradicted the discriminator's ability to select between multiple schema options.

**Correction:** Removed the `type: object` declaration and let the discriminator handle the schema selection via the `oneOf` union, which properly allows the condition to be one of the defined types.

Fixed in: `sovd-interfaces/triggers/types.yaml` line 122

## Rust Handler Fixes

### Missing Tags Fields
The generated structs needed a `tags` field that wasn't being set. Added `tags: None` in two places in the handlers.

Fixed in: `sovd-handlers/src/lib.rs` lines 270 and 378

### Non-existent Fields
The code tried to access `relatedapps` and `relatedcomponents` fields that don't actually exist in the schema. Commented those lines out.

Fixed in: `sovd-handlers/src/lib.rs` lines 1314-1316

## Server Implementation Fixes

### Wrong Return Type
The data categories response was returning simple strings when it should return proper struct instances with the category info.

Fixed in: `sovd-server/src/sovd_server.rs` line 700-721

### Missing Method Parameters
Added missing `tags`, `from_timestamp`, and `to_timestamp` parameters to 10 different methods to match what the OpenAPI trait expects. Rust is strict about method signatures matching exactly.

### Wrong Parameter Type
One method was using `EntityCollectionEntityIdLocksPostRequest` when it should use `EntityCollectionEntityIdLocksLockIdPutRequest`.

Fixed in: `sovd-server/src/sovd_server.rs` line 2386

### Missing Trait Implementations
Had to implement 31 methods that were defined in the OpenAPI spec but not implemented in the server. Added placeholder implementations that return "NotImplemented" errors for:
- Clear data operations (5 methods)
- Cyclic subscriptions (5 methods)
- Logs (1 method)
- Scripts execution (10 methods)
- Entity status management (6 methods)
- Triggers (4 methods)

These can be properly implemented later (probably will be added in future contribution commits)

### Response Type Imports
Added all the necessary imports for the 31 response types so the compiler knows what they are.

Fixed in: `sovd-server/src/sovd_server.rs` lines 18-61


## The Regex Problem

This was the trickiest issue. After fixing all the compilation errors, the server would crash immediately on startup with a regex error.

**The Problem:** The ASAM SOVD spec uses hyphenated parameter names like `area-id`, `component-id`, `entity-collection`, etc. The OpenAPI generator creates regex patterns from these, resulting in stuff like:
```
^/v1/areas/(?P<area-id>[^/?#]*)$
```

But Rust's regex engine doesn't allow hyphens in capture group names! They can only contain letters, numbers, and underscores.

**The Solution:** We can't change the parameter names in the OpenAPI specs because they're part of the official ASAM SOVD standard. So instead, we added a post-processing step to the build script.

The `fix_regex_patterns()` function in `build.sh` runs after code generation and uses sed to:
1. Convert regex patterns: `(?P<area-id>` → `(?P<area_id>`
2. Convert the accessor code: `path_params["area-id"]` → `path_params["area_id"]`

This preserves the standard-compliant names in the OpenAPI specs while making the generated Rust code actually work.

Fixed parameters: `area-id`, `component-id`, `data-id`, `script-id`, `execution-id`, `operation-id`, `entity-collection`, `entity-id`, `lock-id`


## Files That Were Changed

**Build script:**
- `build.sh` - Added the regex fix function

**OpenAPI specs:**
- `sovd-interfaces/scripts/scripts.yaml`
- `sovd-interfaces/cyclic-subscriptions/types.yaml`
- `sovd-interfaces/capability-description/capability-description.yaml`
- `sovd-interfaces/triggers/types.yaml`

**Rust code:**
- `sovd-handlers/src/lib.rs`
- `sovd-server/src/sovd_server.rs`

---
