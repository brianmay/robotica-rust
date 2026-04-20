// Minimal reproduction of rrule treating DATE-only events using the system's local timezone
// offset (+11:00 here, during daylight savings) instead of the calendar's X-WR-TIMEZONE.
// Behavior varies based on where the code runs - non-deterministic across systems.
// Run with: cargo run --example rrule_date_bug

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::America::New_York;
use icalendar::{Calendar as IcalCalendar, Component, EventLike};

fn main() {
    // Create a simple calendar with one all-day event
    let ics = r#"BEGIN:VCALENDAR
X-WR-TIMEZONE:America/New_York
BEGIN:VEVENT
DTSTART;VALUE=DATE:20260401
DTEND;VALUE=DATE:20260402
RRULE:FREQ=DAILY;COUNT=3
SUMMARY:All Day Event
END:VEVENT
END:VCALENDAR"#;

    let calendar = ics.parse::<IcalCalendar>().unwrap();

    println!("Calendar timezone: America/New_York (UTC-4 in April)\n");

    for component in calendar.iter() {
        if let icalendar::CalendarComponent::Event(event) = component {
            let summary = event.get_summary().unwrap_or("none");
            let dtstart = event.get_start();

            println!("Event: {}", summary);
            println!("DTSTART as parsed: {:?}", dtstart);

            // Show what rrule generates
            if let Ok(rrule_set) = event.get_recurrence() {
                let results = rrule_set.clone().all(10);
                println!("\nRRULE occurrences ({} total):", results.dates.len());
                for (i, date) in results.dates.iter().enumerate() {
                    let utc_date: DateTime<Utc> = date.with_timezone(&Utc);
                    let ny_date = New_York.from_utc_datetime(&utc_date.naive_utc());
                    let utc_date_only = date.date_naive();

                    println!(
                        "  [{}] RRULE: {:?} -> UTC: {}, NY: {}, Date only: {}",
                        i, date, utc_date, ny_date, utc_date_only
                    );
                }
            }

            // What we expect
            println!("\nExpected behavior:");
            println!("  April 1, 2, 3 (local dates in New York timezone)");
            println!("  April 1 00:00 NY = April 1 04:00 UTC");
            println!("  April 2 00:00 NY = April 2 04:00 UTC");
            println!("  April 3 00:00 NY = April 3 04:00 UTC");

            // What rrule actually does
            println!("\nActual rrule behavior:");
            println!("  Uses system's local timezone offset (varies by system!)");
            println!("  April 1 00:00+11:00 = March 31 13:00 UTC = March 31 09:00 NY (wrong day!)");
        }
    }
}
