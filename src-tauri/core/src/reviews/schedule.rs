// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! When scheduled reviews run, ported from the previous app's `bff/src/services/review-schedule.ts`.
//! The rule is "the most recent occurrence wins": each enabled kind finds the latest past instant
//! matching its time (and weekday) and runs if its last run predates it. That covers catching up
//! after sleep or a closed app; only the most recent occurrence is caught up. When both kinds are
//! due the daily runs first, because the weekly review reads the daily blocks. The target is
//! pinned from the due instant, not from now, so a late catch-up still reviews the day it was for.
//! Everything is local wall-clock time; DST jumps are not adjusted for.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

use super::target::{week_label, Target};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewKind {
    Daily,
    Weekly,
    /// The daily brief (D-20261118-14): today's, not a review of the day before.
    Brief,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Weekday {
    Sun,
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
}

impl Weekday {
    fn chrono(self) -> chrono::Weekday {
        match self {
            Weekday::Sun => chrono::Weekday::Sun,
            Weekday::Mon => chrono::Weekday::Mon,
            Weekday::Tue => chrono::Weekday::Tue,
            Weekday::Wed => chrono::Weekday::Wed,
            Weekday::Thu => chrono::Weekday::Thu,
            Weekday::Fri => chrono::Weekday::Fri,
            Weekday::Sat => chrono::Weekday::Sat,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleSettings {
    pub daily_enabled: bool,
    /// `HH:MM`, 24-hour, local.
    pub daily_time: String,
    pub weekly_enabled: bool,
    pub weekly_day: Weekday,
    pub weekly_time: String,
    /// The daily brief: off unless turned on, at 07:00 unless set.
    #[serde(default)]
    pub brief_enabled: bool,
    #[serde(default = "brief_time")]
    pub brief_time: String,
}

fn brief_time() -> String {
    "07:00".into()
}

/// When you do the guided weekly review (Today reminds you then), not a job: Friday 16:00 unless
/// set (Settings › Jobs & schedule, stored as `weeklyReview`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WeeklyReview {
    pub day: Weekday,
    /// `HH:MM`, 24-hour, local.
    pub time: String,
}

impl Default for WeeklyReview {
    fn default() -> Self {
        WeeklyReview { day: Weekday::Fri, time: "16:00".into() }
    }
}

impl WeeklyReview {
    /// Why it can't be saved, in words for the user.
    pub fn validate(&self) -> Result<(), String> {
        parse_time(&self.time).map(|_| ()).ok_or_else(|| "weeklyReview.time must be HH:MM (24-hour)".to_string())
    }
}

impl Default for ScheduleSettings {
    /// Off, at the previous app's default times.
    fn default() -> Self {
        ScheduleSettings {
            daily_enabled: false,
            daily_time: "06:30".into(),
            weekly_enabled: false,
            weekly_day: Weekday::Fri,
            weekly_time: "16:00".into(),
            brief_enabled: false,
            brief_time: brief_time(),
        }
    }
}

impl ScheduleSettings {
    /// Why the settings can't be saved, in words for the user.
    pub fn validate(&self) -> Result<(), String> {
        for (k, v) in [("dailyTime", &self.daily_time), ("weeklyTime", &self.weekly_time), ("briefTime", &self.brief_time)] {
            if parse_time(v).is_none() {
                return Err(format!("summaries.{k} must be HH:MM (24-hour)"));
            }
        }
        Ok(())
    }

    /// Which kinds' schedules differ in `next`. A changed (or newly enabled) kind has its last run
    /// stamped as now, so it first fires at the next occurrence: enabling 06:30 at 10:00 shouldn't
    /// start a review at 10:00.
    pub fn changed(&self, next: &ScheduleSettings) -> Vec<ReviewKind> {
        let mut out = Vec::new();
        if self.daily_enabled != next.daily_enabled || self.daily_time != next.daily_time {
            out.push(ReviewKind::Daily);
        }
        if self.weekly_enabled != next.weekly_enabled || self.weekly_day != next.weekly_day || self.weekly_time != next.weekly_time {
            out.push(ReviewKind::Weekly);
        }
        if self.brief_enabled != next.brief_enabled || self.brief_time != next.brief_time {
            out.push(ReviewKind::Brief);
        }
        out
    }
}

/// `HH:MM` with a two-digit hour 00–23 and minute 00–59.
pub fn parse_time(s: &str) -> Option<NaiveTime> {
    let b = s.as_bytes();
    if b.len() != 5 || b[2] != b':' || !b.iter().enumerate().all(|(i, c)| i == 2 || c.is_ascii_digit()) {
        return None;
    }
    NaiveTime::from_hms_opt(s[..2].parse().ok()?, s[3..].parse().ok()?, 0)
}

/// An invalid time counts as midnight, as there.
fn at_time(day: NaiveDate, time: &str) -> NaiveDateTime {
    day.and_time(parse_time(time).unwrap_or(NaiveTime::MIN))
}

/// The latest instant at or before `now` that is `time` o'clock, on `weekday` when one is given.
pub fn last_occurrence(now: NaiveDateTime, time: &str, weekday: Option<Weekday>) -> NaiveDateTime {
    let mut c = at_time(now.date(), time);
    if c > now {
        c -= Duration::days(1);
    }
    if let Some(w) = weekday {
        while c.weekday() != w.chrono() {
            c -= Duration::days(1);
        }
    }
    c
}

/// The first instant strictly after `now` matching `time` (and `weekday`).
pub fn next_occurrence(now: NaiveDateTime, time: &str, weekday: Option<Weekday>) -> NaiveDateTime {
    let mut c = at_time(now.date(), time);
    if c <= now {
        c += Duration::days(1);
    }
    if let Some(w) = weekday {
        while c.weekday() != w.chrono() {
            c += Duration::days(1);
        }
    }
    c
}

/// `2026-W36`: weeks run Monday to Sunday and belong to the year holding their Thursday.
pub fn iso_week(day: NaiveDate) -> String {
    week_label(day)
}

/// The ISO week a weekly run due at `due` reviews, and whether that's the week before. Monday and
/// Tuesday runs review the week just ended; from Wednesday on, the current week.
pub fn weekly_target(due: NaiveDateTime) -> (String, bool) {
    let previous = matches!(due.weekday(), chrono::Weekday::Mon | chrono::Weekday::Tue);
    let day = if previous { due.date() - Duration::days(7) } else { due.date() };
    (iso_week(day), previous)
}

/// What a run due at `due` reviews: the day before for a daily (a 06:30 run reviews yesterday),
/// the week [`weekly_target`] picks for a weekly.
pub fn target_for(kind: ReviewKind, due: NaiveDateTime) -> Target {
    match kind {
        ReviewKind::Daily => Target::Day(due.date() - Duration::days(1)),
        ReviewKind::Weekly => {
            let day = if weekly_target(due).1 { due.date() - Duration::days(7) } else { due.date() };
            Target::week_of(day)
        }
        ReviewKind::Brief => Target::Day(due.date()),
    }
}

/// When each kind last ran: the occurrence it satisfied, or the time its schedule was changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LastRun {
    pub daily: Option<NaiveDateTime>,
    pub weekly: Option<NaiveDateTime>,
    pub brief: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Due {
    pub kind: ReviewKind,
    /// The occurrence this run satisfies; stamp it as the kind's last run.
    #[serde(serialize_with = "super::ser_time")]
    pub due_at: NaiveDateTime,
}

/// Which kinds are due at `now`, daily first, and the occurrence each satisfies.
pub fn compute_due(s: &ScheduleSettings, last: &LastRun, now: NaiveDateTime) -> Vec<Due> {
    let mut due = Vec::new();
    let mut check = |kind, enabled: bool, occurrence: NaiveDateTime, last: Option<NaiveDateTime>| {
        if enabled && last.is_none_or(|l| l < occurrence) {
            due.push(Due { kind, due_at: occurrence });
        }
    };
    check(ReviewKind::Daily, s.daily_enabled, last_occurrence(now, &s.daily_time, None), last.daily);
    check(ReviewKind::Weekly, s.weekly_enabled, last_occurrence(now, &s.weekly_time, Some(s.weekly_day)), last.weekly);
    check(ReviewKind::Brief, s.brief_enabled, last_occurrence(now, &s.brief_time, None), last.brief);
    due
}

/// Of what's due, what to start now given what's `running`. A kind already running waits; the
/// weekly also waits while a daily is due or running, and stays due until then.
pub fn to_start(due: &[Due], running: &[ReviewKind]) -> Vec<Due> {
    let daily_busy = running.contains(&ReviewKind::Daily) || due.iter().any(|d| d.kind == ReviewKind::Daily);
    due.iter().filter(|d| !running.contains(&d.kind) && !(d.kind == ReviewKind::Weekly && daily_busy)).copied().collect()
}

/// The next time each kind fires, for Settings; None when it's off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NextDue {
    #[serde(serialize_with = "super::ser_time_opt")]
    pub daily: Option<NaiveDateTime>,
    #[serde(serialize_with = "super::ser_time_opt")]
    pub weekly: Option<NaiveDateTime>,
    #[serde(serialize_with = "super::ser_time_opt")]
    pub brief: Option<NaiveDateTime>,
}

pub fn next_due(s: &ScheduleSettings, now: NaiveDateTime) -> NextDue {
    NextDue {
        daily: s.daily_enabled.then(|| next_occurrence(now, &s.daily_time, None)),
        weekly: s.weekly_enabled.then(|| next_occurrence(now, &s.weekly_time, Some(s.weekly_day))),
        brief: s.brief_enabled.then(|| next_occurrence(now, &s.brief_time, None)),
    }
}

#[cfg(test)]
mod tests {
    //! review-schedule.test.ts's clock arithmetic, case for case.

    use super::*;

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap()
    }

    fn day(y: i32, mo: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, mo, d).unwrap()
    }

    fn on() -> ScheduleSettings {
        ScheduleSettings { daily_enabled: true, weekly_enabled: true, ..Default::default() }
    }

    // 2026-09-08 is a Tuesday.
    #[test]
    fn last_occurrence_is_todays_time_once_passed() {
        assert_eq!(last_occurrence(at(2026, 9, 8, 10, 0), "06:30", None), at(2026, 9, 8, 6, 30));
        assert_eq!(last_occurrence(at(2026, 9, 8, 6, 0), "06:30", None), at(2026, 9, 7, 6, 30));
        // Exactly on time counts as passed.
        assert_eq!(last_occurrence(at(2026, 9, 8, 6, 30), "06:30", None), at(2026, 9, 8, 6, 30));
    }

    #[test]
    fn last_occurrence_walks_back_to_the_weekday() {
        assert_eq!(last_occurrence(at(2026, 9, 8, 10, 0), "16:00", Some(Weekday::Fri)), at(2026, 9, 4, 16, 0));
        // Friday 15:00: today's slot hasn't come, so a week ago Friday.
        assert_eq!(last_occurrence(at(2026, 9, 4, 15, 0), "16:00", Some(Weekday::Fri)), at(2026, 8, 28, 16, 0));
    }

    #[test]
    fn next_occurrence_is_the_first_future_slot() {
        assert_eq!(next_occurrence(at(2026, 9, 8, 10, 0), "06:30", None), at(2026, 9, 9, 6, 30));
        assert_eq!(next_occurrence(at(2026, 9, 8, 10, 0), "16:00", Some(Weekday::Fri)), at(2026, 9, 11, 16, 0));
        assert_eq!(next_occurrence(at(2026, 9, 11, 16, 0), "16:00", Some(Weekday::Fri)), at(2026, 9, 18, 16, 0));
    }

    #[test]
    fn iso_weeks_at_year_boundaries() {
        assert_eq!(iso_week(day(2026, 9, 8)), "2026-W37");
        assert_eq!(iso_week(day(2026, 1, 1)), "2026-W01");
        assert_eq!(iso_week(day(2027, 1, 1)), "2026-W53");
        assert_eq!(iso_week(day(2024, 12, 30)), "2025-W01");
    }

    #[test]
    fn targets_are_yesterday_and_the_due_week() {
        assert_eq!(target_for(ReviewKind::Daily, at(2026, 9, 8, 6, 30)).label(), "2026-09-07");
        assert_eq!(target_for(ReviewKind::Daily, at(2026, 9, 1, 6, 30)).label(), "2026-08-31");
        assert_eq!(target_for(ReviewKind::Weekly, at(2026, 9, 4, 16, 0)).label(), "2026-W36");
        // Monday and Tuesday runs review the week just ended.
        assert_eq!(target_for(ReviewKind::Weekly, at(2026, 9, 7, 7, 30)).label(), "2026-W36");
        assert_eq!(target_for(ReviewKind::Weekly, at(2026, 9, 8, 7, 30)).label(), "2026-W36");
        assert_eq!(target_for(ReviewKind::Weekly, at(2026, 9, 9, 7, 30)).label(), "2026-W37");
        assert_eq!(weekly_target(at(2026, 9, 7, 0, 0)), ("2026-W36".into(), true));
        assert_eq!(weekly_target(at(2026, 9, 13, 0, 0)), ("2026-W37".into(), false));
        assert_eq!(target_for(ReviewKind::Weekly, at(2026, 9, 8, 7, 30)), Target::Week { year: 2026, week: 36 });
        assert_eq!(target_for(ReviewKind::Daily, at(2026, 1, 1, 6, 30)), Target::Day(day(2025, 12, 31)));
    }

    #[test]
    fn due_once_per_occurrence_and_only_when_enabled() {
        let none = LastRun::default();
        let kinds = |d: Vec<Due>| d.into_iter().map(|d| d.kind).collect::<Vec<_>>();
        assert_eq!(kinds(compute_due(&on(), &none, at(2026, 9, 8, 10, 0))), [ReviewKind::Daily, ReviewKind::Weekly]);
        let ran = LastRun { daily: Some(at(2026, 9, 8, 6, 30)), weekly: Some(at(2026, 9, 4, 16, 0)), brief: None };
        assert!(compute_due(&on(), &ran, at(2026, 9, 8, 10, 0)).is_empty());
        // A later stamp (the settings just changed) holds it off too.
        let later = LastRun { daily: Some(at(2026, 9, 8, 9, 0)), weekly: None, brief: None };
        assert!(compute_due(&ScheduleSettings { weekly_enabled: false, ..on() }, &later, at(2026, 9, 8, 10, 0)).is_empty());
        // Missed yesterday's 06:30 and today's hasn't come: yesterday's is due, once.
        assert_eq!(
            compute_due(
                &ScheduleSettings { weekly_enabled: false, ..on() },
                &LastRun { daily: Some(at(2026, 9, 5, 6, 30)), weekly: None, brief: None },
                at(2026, 9, 8, 6, 0)
            ),
            [Due { kind: ReviewKind::Daily, due_at: at(2026, 9, 7, 6, 30) }]
        );
        assert!(compute_due(&ScheduleSettings::default(), &none, at(2026, 9, 8, 6, 0)).is_empty());
    }

    #[test]
    fn daily_before_weekly() {
        let due = compute_due(&on(), &LastRun::default(), at(2026, 9, 8, 10, 0));
        assert_eq!(to_start(&due, &[]), [due[0]]);
        // Still running: the weekly keeps waiting, and stays due.
        let after = compute_due(&on(), &LastRun { daily: Some(due[0].due_at), weekly: None, brief: None }, at(2026, 9, 8, 10, 1));
        assert!(to_start(&after, &[ReviewKind::Daily]).is_empty());
        // The daily finished (or failed): the weekly goes.
        assert_eq!(to_start(&after, &[]), [Due { kind: ReviewKind::Weekly, due_at: at(2026, 9, 4, 16, 0) }]);
        assert!(to_start(&after, &[ReviewKind::Weekly]).is_empty());
    }

    #[test]
    fn settings() {
        let s = ScheduleSettings { daily_time: "6:30".into(), ..on() };
        assert!(s.validate().unwrap_err().contains("HH:MM"));
        assert!(ScheduleSettings { weekly_time: "24:00".into(), ..on() }.validate().is_err());
        assert!(on().validate().is_ok());
        assert_eq!(
            ScheduleSettings::default().changed(&ScheduleSettings { daily_enabled: true, ..Default::default() }),
            [ReviewKind::Daily]
        );
        assert_eq!(on().changed(&ScheduleSettings { weekly_day: Weekday::Sun, ..on() }), [ReviewKind::Weekly]);
        assert!(WeeklyReview { time: "9am".into(), ..Default::default() }.validate().is_err());
        assert!(WeeklyReview::default().validate().is_ok());
        let json: ScheduleSettings = serde_json::from_str(
            r#"{"dailyEnabled":true,"dailyTime":"07:15","weeklyEnabled":false,"weeklyDay":"sun","weeklyTime":"16:00"}"#,
        )
        .unwrap();
        assert_eq!(json.weekly_day, Weekday::Sun);
        let review: WeeklyReview = serde_json::from_str(r#"{"time":"09:00"}"#).unwrap();
        assert_eq!((review.day, review.time.as_str()), (Weekday::Fri, "09:00"));
        assert_eq!(
            next_due(&on(), at(2026, 9, 8, 10, 0)),
            NextDue { daily: Some(at(2026, 9, 9, 6, 30)), weekly: Some(at(2026, 9, 11, 16, 0)), brief: None }
        );
    }

    #[test]
    fn the_brief_is_due_each_day_at_its_time_and_is_for_that_day() {
        let s = ScheduleSettings { brief_enabled: true, brief_time: "07:00".into(), ..Default::default() };
        let none = LastRun::default();
        assert!(compute_due(&s, &none, at(2026, 11, 18, 6, 59)).iter().any(|d| d.kind == ReviewKind::Brief), "never run: one is due");
        let due = compute_due(&s, &LastRun { brief: Some(at(2026, 11, 17, 7, 0)), ..none }, at(2026, 11, 18, 7, 1));
        assert_eq!(due, vec![Due { kind: ReviewKind::Brief, due_at: at(2026, 11, 18, 7, 0) }]);
        assert!(compute_due(&s, &LastRun { brief: Some(at(2026, 11, 18, 7, 0)), ..none }, at(2026, 11, 18, 9, 0)).is_empty());
        assert_eq!(target_for(ReviewKind::Brief, at(2026, 11, 18, 7, 0)), Target::Day(day(2026, 11, 18)));
        assert_eq!(next_due(&s, at(2026, 11, 18, 9, 0)).brief, Some(at(2026, 11, 19, 7, 0)));
        // Off by default, and an old settings file without it reads as off at 07:00.
        let old: ScheduleSettings = serde_json::from_str(
            r#"{"dailyEnabled":true,"dailyTime":"06:30","weeklyEnabled":false,"weeklyDay":"fri","weeklyTime":"16:00"}"#,
        )
        .unwrap();
        assert_eq!((old.brief_enabled, old.brief_time.as_str()), (false, "07:00"));
        assert_eq!(ScheduleSettings::default().changed(&s), vec![ReviewKind::Brief]);
        assert!(ScheduleSettings { brief_time: "7am".into(), ..s }.validate().is_err());
    }
}
