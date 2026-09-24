//! Unix timestamps for trash retention and backups.

use crate::*;

pub(crate) fn unix_timestamp() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| format!("System time could not be read: {error}"))
}
