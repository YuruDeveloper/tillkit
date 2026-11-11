use std::time::{SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: u64 = 86_400;

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

pub fn format_utc(unix: u64) -> String {
    let (year, month, day) = civil_date(unix / SECONDS_PER_DAY);
    let seconds = unix % SECONDS_PER_DAY;
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}Z",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}

fn civil_date(days_since_epoch: u64) -> (u64, u64, u64) {
    let shifted = days_since_epoch + 719_468;
    let era = shifted / 146_097;
    let day_of_era = shifted % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::format_utc;

    #[test]
    fn known_instants_format_as_utc() {
        assert_eq!(format_utc(0), "1970-01-01 00:00Z");
        assert_eq!(format_utc(951_782_400), "2000-02-29 00:00Z");
        assert_eq!(format_utc(951_868_799), "2000-02-29 23:59Z");
        assert_eq!(format_utc(951_868_800), "2000-03-01 00:00Z");
        assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13Z");
        assert_eq!(format_utc(1_675_555_200), "2023-02-05 00:00Z");
    }
}
