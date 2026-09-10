use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, Utc};
use icalendar::{Calendar, Class, Component, Event, EventLike, EventStatus};
use reqwest::Url;
use serde::Deserialize;

use crate::Error;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vacation {
    id: u64,
    worker_name: String,
    date_start: NaiveDate,
    date_end: NaiveDate,
    #[serde(default)]
    department_name: Option<String>,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    moderation_status: Option<String>,
}

fn text(value: &str) -> String {
    value
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .collect()
}

pub fn render(vacations: Vec<Vacation>, origin: &Url) -> Result<String, Error> {
    let mut unique = BTreeMap::new();
    for vacation in vacations {
        if vacation.id == 0
            || vacation.worker_name.trim().is_empty()
            || vacation.date_start > vacation.date_end
            || !(1..=9998).contains(&vacation.date_start.year())
            || !(1..=9998).contains(&vacation.date_end.year())
        {
            return Err(Error::upstream());
        }
        // The same cross-year vacation may be returned by both year endpoints.
        unique.insert(vacation.id, vacation);
    }
    let mut calendar = Calendar::new();
    calendar.name("Vacationplanner");
    let timestamp = Utc::now();
    for vacation in unique.into_values() {
        // Vacationplanner's final day is inclusive; iCalendar's DTEND is exclusive.
        let end = vacation.date_end.succ_opt().ok_or_else(Error::upstream)?;
        let uid = format!(
            "vacation-{}@{}",
            vacation.id,
            origin.origin().ascii_serialization().replace("://", "-")
        );
        let mut description = Vec::new();
        if let Some(department) = vacation.department_name.filter(|value| !value.is_empty()) {
            description.push(format!("Department: {}", text(&department)));
        }
        if let Some(comment) = vacation.comment.filter(|value| !value.is_empty()) {
            description.push(text(&comment));
        }
        let status = match vacation.moderation_status.as_deref() {
            Some("approved") => Some(EventStatus::Confirmed),
            Some("on_moderation") => Some(EventStatus::Tentative),
            Some("rejected") => Some(EventStatus::Cancelled),
            _ => None,
        };
        if let Some(moderation) = vacation.moderation_status {
            description.push(format!("Moderation: {}", text(&moderation)));
        }
        let mut event = Event::new();
        event
            .uid(&uid)
            .timestamp(timestamp)
            .starts(vacation.date_start)
            .ends(end)
            .summary(&format!("{} — Vacation", text(&vacation.worker_name)))
            .description(&description.join("\n"))
            .class(Class::Private);
        if let Some(status) = status {
            event.status(status);
        }
        calendar.push(event);
    }
    Ok(calendar.to_string())
}
