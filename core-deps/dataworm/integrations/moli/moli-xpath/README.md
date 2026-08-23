# moli-xpath

`moli-xpath` is a Moli-maintained fork of Servo's
`components/xpath` crate.

Source:

- Upstream project: https://github.com/servo/servo
- Upstream path: `components/xpath`
- Upstream license: MPL-2.0

The copied Servo XPath evaluator files keep their MPL-2.0 source headers. The
crate includes a local copy of the MPL-2.0 text in `LICENSE-MPL-2.0`.

Moli changes:

- removed Servo workspace metadata;
- removed `malloc_size_of` / `malloc_size_of_derive` because Moli does
  not use Servo's memory reporting hooks here;
- added a lightweight snapshot DOM adapter for detached/foreign object-tree
  bridges; the V8 renderer's normal `Document.evaluate()` path wraps its live
  DOM directly and uses this crate's generic DOM traits.

The MPL-2.0 obligation is file-level. Files copied from Servo, and files that
modify copied Servo code, remain MPL-2.0. Other Moli crates and files are
separate larger-work files and keep their own license terms.
