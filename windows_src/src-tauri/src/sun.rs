//! Sunrise/sunset calculation.

use crate::*;

// The classic "Sunrise/Sunset Algorithm" (Almanac for Computers, 1990, as
// popularized by edwilliams.org/sunrise_sunset_algorithm.htm). Pure and
// self-contained so it can be unit tested against known reference values
// without needing a real date or timezone. Returns the event as UTC
// minutes-since-midnight, or `None` when the sun does not rise/set that day
// at that latitude (polar day/night).
pub(crate) fn sun_event_utc_minutes(day_of_year: u32, latitude: f64, longitude: f64, is_sunrise: bool) -> Option<f64> {
    const ZENITH: f64 = 90.833; // official zenith for sunrise/sunset (includes refraction + solar radius)
    let to_radians = std::f64::consts::PI / 180.0;
    let to_degrees = 180.0 / std::f64::consts::PI;

    let lng_hour = longitude / 15.0;
    let hour_anchor = if is_sunrise { 6.0 } else { 18.0 };
    let t = day_of_year as f64 + ((hour_anchor - lng_hour) / 24.0);

    let mean_anomaly = (0.9856 * t) - 3.289;

    let mut true_longitude = mean_anomaly
        + (1.916 * (mean_anomaly * to_radians).sin())
        + (0.020 * (2.0 * mean_anomaly * to_radians).sin())
        + 282.634;
    true_longitude = ((true_longitude % 360.0) + 360.0) % 360.0;

    let mut right_ascension = to_degrees * (0.91764 * (true_longitude * to_radians).tan()).atan();
    right_ascension = ((right_ascension % 360.0) + 360.0) % 360.0;
    let longitude_quadrant = (true_longitude / 90.0).floor() * 90.0;
    let ascension_quadrant = (right_ascension / 90.0).floor() * 90.0;
    right_ascension += longitude_quadrant - ascension_quadrant;
    right_ascension /= 15.0;

    let sin_declination = 0.39782 * (true_longitude * to_radians).sin();
    let cos_declination = sin_declination.asin().cos();

    let cos_hour_angle = ((ZENITH * to_radians).cos() - (sin_declination * (latitude * to_radians).sin()))
        / (cos_declination * (latitude * to_radians).cos());
    if !(-1.0..=1.0).contains(&cos_hour_angle) {
        return None;
    }

    let hour_angle_degrees = if is_sunrise {
        360.0 - to_degrees * cos_hour_angle.acos()
    } else {
        to_degrees * cos_hour_angle.acos()
    };
    let hour_angle = hour_angle_degrees / 15.0;

    let local_mean_time = hour_angle + right_ascension - (0.06571 * t) - 6.622;

    let mut utc_hours = local_mean_time - lng_hour;
    utc_hours = ((utc_hours % 24.0) + 24.0) % 24.0;

    Some(utc_hours * 60.0)
}

// Combines a UTC sun-event time with a UTC offset to get local (hour, minute).
pub(crate) fn sun_event_local_time(utc_minutes: f64, utc_offset_minutes: i64) -> (u32, u32) {
    let local = utc_minutes + utc_offset_minutes as f64;
    let normalized = (((local % 1440.0) + 1440.0) % 1440.0).round() as i64 % 1440;
    ((normalized / 60) as u32, (normalized % 60) as u32)
}

// Resolves what time a timed automation should fire at *today*: a fixed
// clock time, or today's actual sunrise/sunset for the configured region.
// `None` for a sun trigger means the sun does not rise/set there today
// (polar day/night) - the caller should just skip it for today.
pub(crate) fn resolve_trigger_time_today(
    entry: &TimedAutomation,
    today_day_of_year: u32,
    utc_offset_minutes: i64,
) -> Result<Option<(u32, u32)>, String> {
    match entry.trigger_kind.as_str() {
        "clock" => Ok(Some(parse_time_of_day(&entry.time)?)),
        "sunrise" | "sunset" => {
            let location = load_sun_location()?;
            let (latitude, longitude) = sun_region_coordinates(&location.region)
                .ok_or_else(|| format!("\"{}\" is not a known region.", location.region))?;
            let is_sunrise = entry.trigger_kind == "sunrise";
            match sun_event_utc_minutes(today_day_of_year, latitude, longitude, is_sunrise) {
                Some(utc_minutes) => Ok(Some(sun_event_local_time(utc_minutes, utc_offset_minutes))),
                None => Ok(None),
            }
        }
        other => Err(format!("\"{}\" is not a valid trigger.", other)),
    }
}
