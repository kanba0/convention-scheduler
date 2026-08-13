/**
 * Hand-written mirrors of the API's serialized shapes, one per Rust struct.
 *
 * Dates and times stay strings: program times are venue wall clock with no zone,
 * and a JS `Date` would silently attach the browser's offset to them.
 */

/** `src/conventions.rs`. */
export type Convention = {
  id: string;
  name: string;
  starts_on: string;
  ends_on: string;
  created_at: string;
  updated_at: string;
};