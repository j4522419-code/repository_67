//! Dates and times, written the way this PC's region settings say.

use windows::core::PCWSTR;
use windows::Win32::Globalization::{
    GetDateFormatEx, GetTimeFormatEx, DATE_SHORTDATE, TIME_NOSECONDS,
};

/// Today's date and the time now, like `29/09/2026` and `14:05`.
pub fn date_and_time() -> (String, String) {
    let mut date = [0u16; 128];
    let mut time = [0u16; 128];
    // A null locale name means the user's own settings; no date or time
    // given means now.
    let (date_len, time_len) = unsafe {
        (
            GetDateFormatEx(
                PCWSTR::null(),
                DATE_SHORTDATE,
                None,
                PCWSTR::null(),
                Some(&mut date),
                PCWSTR::null(),
            ),
            GetTimeFormatEx(
                PCWSTR::null(),
                TIME_NOSECONDS,
                None,
                PCWSTR::null(),
                Some(&mut time),
            ),
        )
    };
    (written(&date, date_len), written(&time, time_len))
}

/// The text a formatting call wrote; its count includes the ending zero.
fn written(buffer: &[u16], count: i32) -> String {
    match usize::try_from(count) {
        Ok(count) if count > 0 => String::from_utf16_lossy(&buffer[..count - 1]),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_the_date_and_time() {
        let (date, time) = date_and_time();
        println!("date: {date}, time: {time}");
        assert!(date.chars().any(|c| c.is_ascii_digit()), "{date}");
        assert!(time.chars().any(|c| c.is_ascii_digit()), "{time}");
    }
}
