//! Local date/time helpers (chrono) used by timed automations.

use crate::*;

pub(crate) fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("System time could not be read: {}", error))
}

// One consistent snapshot of the local wall clock. Taking every field from a
// single `Local::now()` avoids the date and time drifting apart when a check
// runs right around midnight. The weekday is taken from WEEKDAYS directly,
// so it never depends on the system locale.
pub(crate) struct LocalNow {
    pub(crate) hour: u32,
    pub(crate) minute: u32,
    pub(crate) date: String,
    pub(crate) day_of_year: u32,
    pub(crate) weekday: String,
    pub(crate) utc_offset_minutes: i64,
}

pub(crate) fn local_now() -> LocalNow {
    local_snapshot(&Local::now())
}

// Pure conversion, split out so it can be tested with fixed dates/offsets.
pub(crate) fn local_snapshot<Tz: TimeZone>(now: &DateTime<Tz>) -> LocalNow {
    LocalNow {
        hour: now.hour(),
        minute: now.minute(),
        date: format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day()),
        day_of_year: now.ordinal(),
        weekday: WEEKDAYS[now.weekday().num_days_from_monday() as usize].to_string(),
        utc_offset_minutes: i64::from(now.offset().fix().local_minus_utc()) / 60,
    }
}
