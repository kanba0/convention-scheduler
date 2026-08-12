//! The wire and in-memory format for program times. They're venue wall clock, so
//! they carry no offset -- unlike `created_at`/`updated_at`, which stay real instants.

time::serde::format_description!(
    pub local_datetime,
    PrimitiveDateTime,
    "[year]-[month]-[day]T[hour]:[minute]:[second]"
);
