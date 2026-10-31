//! Day/Night scenes: when does the look change, and what counts as a change?
//!
//! Pure, so the awkward cases are tested: boundaries that wrap past midnight, equal times,
//! clocks that jump (daylight saving, time zone change, resume from sleep), and the rule that
//! a manual scene choice stays until the *next* boundary.
//!
//! The host arms one timer for the next boundary and re-evaluates on resume and on time or
//! time-zone changes. It never polls, and a timer cannot wake the PC.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Day,
    Night,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Times {
    /// Minutes after local midnight when day begins / night begins.
    pub day_starts: u16,
    pub night_starts: u16,
}

pub fn parse_minutes(text: &str) -> Result<u16, String> {
    let bad = || format!("“{text}” is not a time like 07:00");
    let (hours, minutes) = text.split_once(':').ok_or_else(bad)?;
    // Digits only: `u16::parse` would also accept a leading `+`.
    let digits = |part: &str| part.len() == 2 && part.bytes().all(|b| b.is_ascii_digit());
    if !digits(hours) || !digits(minutes) {
        return Err(bad());
    }
    let hours: u16 = hours.parse().map_err(|_| bad())?;
    let minutes: u16 = minutes.parse().map_err(|_| bad())?;
    if hours > 23 || minutes > 59 {
        return Err(bad());
    }
    Ok(hours * 60 + minutes)
}

pub fn parse_times(day_starts: &str, night_starts: &str) -> Result<Times, String> {
    let day = parse_minutes(day_starts)?;
    let night = parse_minutes(night_starts)?;
    if day == night {
        return Err("day and night cannot start at the same time".into());
    }
    Ok(Times {
        day_starts: day,
        night_starts: night,
    })
}

pub fn phase_at(minute_of_day: u16, times: &Times) -> Phase {
    let minute = minute_of_day % 1440;
    let (day, night) = (times.day_starts, times.night_starts);
    let in_day = if day < night {
        minute >= day && minute < night
    } else {
        // Day runs through midnight (e.g. day 20:00, night 06:00).
        minute >= day || minute < night
    };
    if in_day {
        Phase::Day
    } else {
        Phase::Night
    }
}

/// Milliseconds from now until the phase next changes. Never zero, never more than a day.
pub fn millis_until_change(minute_of_day: u16, second: u16, times: &Times) -> u64 {
    let minute = minute_of_day % 1440;
    let now = u64::from(minute) * 60 + u64::from(second.min(59));
    let mut best = 24 * 3600u64;
    for boundary in [times.day_starts, times.night_starts] {
        let target = u64::from(boundary) * 60;
        let wait = if target > now {
            target - now
        } else {
            target + 24 * 3600 - now
        };
        best = best.min(wait);
    }
    best.max(1) * 1000
}

/// What the scheduler has already acted on. Switching scenes by hand does not touch it, so a
/// manual choice stays until the phase next changes.
#[derive(Debug, Default)]
pub struct Scheduler {
    last: Option<Phase>,
}

impl Scheduler {
    /// Evaluate "now". Returns the phase to apply when it differs from the last one acted on
    /// (including the first evaluation), otherwise `None`.
    pub fn evaluate(&mut self, minute_of_day: u16, times: &Times) -> Option<Phase> {
        let now = phase_at(minute_of_day, times);
        if self.last == Some(now) {
            return None;
        }
        self.last = Some(now);
        Some(now)
    }

    /// Forget what was acted on (the feature was turned off), so turning it on applies the
    /// current phase.
    pub fn reset(&mut self) {
        self.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(day: &str, night: &str) -> Times {
        parse_times(day, night).unwrap()
    }

    #[test]
    fn times_parse_strictly() {
        assert_eq!(parse_minutes("00:00"), Ok(0));
        assert_eq!(parse_minutes("23:59"), Ok(1439));
        for bad in [
            "", "7:00", "07:0", "24:00", "07:60", "07-00", "07:00:00", "ab:cd", "+7:00", " 07:00",
        ] {
            assert!(parse_minutes(bad).is_err(), "{bad:?}");
        }
        assert!(parse_times("07:00", "07:00").is_err());
    }

    #[test]
    fn phases_follow_the_clock_including_across_midnight() {
        let normal = t("07:00", "20:00");
        assert_eq!(phase_at(6 * 60 + 59, &normal), Phase::Night);
        assert_eq!(phase_at(7 * 60, &normal), Phase::Day);
        assert_eq!(phase_at(19 * 60 + 59, &normal), Phase::Day);
        assert_eq!(phase_at(20 * 60, &normal), Phase::Night);
        assert_eq!(phase_at(0, &normal), Phase::Night);
        // A night owl: day starts in the evening and ends in the early morning.
        let owl = t("20:00", "06:00");
        assert_eq!(phase_at(23 * 60, &owl), Phase::Day);
        assert_eq!(phase_at(5 * 60, &owl), Phase::Day);
        assert_eq!(phase_at(12 * 60, &owl), Phase::Night);
    }

    #[test]
    fn the_timer_always_targets_the_next_boundary() {
        let times = t("07:00", "20:00");
        assert_eq!(millis_until_change(6 * 60 + 59, 0, &times), 60_000);
        assert_eq!(millis_until_change(19 * 60 + 59, 30, &times), 30_000);
        // Just after a boundary the next one is the other boundary, not a day away.
        assert_eq!(millis_until_change(7 * 60, 0, &times), 13 * 3600 * 1000);
        // Late evening waits across midnight to 07:00.
        assert_eq!(
            millis_until_change(23 * 60 + 59, 0, &times),
            (7 * 3600 + 60) * 1000
        );
        // Never zero, never more than a day.
        for minute in 0..1440u16 {
            let wait = millis_until_change(minute, 59, &times);
            assert!(
                (1000..=24 * 3600 * 1000).contains(&wait),
                "{minute}: {wait}"
            );
        }
    }

    #[test]
    fn the_first_evaluation_applies_and_repeats_do_nothing() {
        let times = t("07:00", "20:00");
        let mut scheduler = Scheduler::default();
        assert_eq!(scheduler.evaluate(9 * 60, &times), Some(Phase::Day));
        assert_eq!(scheduler.evaluate(9 * 60 + 1, &times), None);
        assert_eq!(scheduler.evaluate(15 * 60, &times), None);
    }

    #[test]
    fn a_manual_choice_stays_until_the_next_boundary() {
        // The scheduler never hears about manual scene changes, so within a phase it stays
        // silent no matter how often it is asked; at the boundary it speaks once.
        let times = t("07:00", "20:00");
        let mut scheduler = Scheduler::default();
        assert_eq!(scheduler.evaluate(10 * 60, &times), Some(Phase::Day));
        for minute in [11 * 60, 14 * 60, 19 * 60 + 59] {
            assert_eq!(scheduler.evaluate(minute, &times), None);
        }
        assert_eq!(scheduler.evaluate(20 * 60, &times), Some(Phase::Night));
        assert_eq!(scheduler.evaluate(20 * 60 + 1, &times), None);
    }

    #[test]
    fn clock_jumps_and_resume_are_handled_by_comparing_phases() {
        let times = t("07:00", "20:00");
        let mut scheduler = Scheduler::default();
        scheduler.evaluate(22 * 60, &times); // night
                                             // Resume the next morning (slept through the 07:00 boundary): one switch to day.
        assert_eq!(scheduler.evaluate(8 * 60, &times), Some(Phase::Day));
        // Daylight saving moves the clock an hour inside the same phase: nothing to do.
        assert_eq!(scheduler.evaluate(9 * 60, &times), None);
        // A time-zone change that lands in the other phase: one switch.
        assert_eq!(scheduler.evaluate(21 * 60, &times), Some(Phase::Night));
    }

    #[test]
    fn turning_the_feature_off_and_on_applies_the_current_phase_again() {
        let times = t("07:00", "20:00");
        let mut scheduler = Scheduler::default();
        scheduler.evaluate(10 * 60, &times);
        scheduler.reset();
        assert_eq!(scheduler.evaluate(10 * 60 + 5, &times), Some(Phase::Day));
    }
}
