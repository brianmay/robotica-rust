#!/usr/bin/env python3
import urllib.request
from datetime import date, datetime, timedelta, timezone

import icalendar
import recurring_ical_events


def read_calendar(ical_string, start_date, end_date):
    calendar = icalendar.Calendar.from_ical(ical_string)
    events = recurring_ical_events.of(calendar).between(start_date, end_date)
    result = []
    for event in events:
        json_data = {}
        for i in event.keys():
            if isinstance(event[i], icalendar.prop.vDDDTypes):
                dt = event[i].dt
                if isinstance(dt, datetime):
                    json_data[i] = dt.astimezone(timezone.utc)
                elif isinstance(dt, date):
                    json_data[i] = dt
                elif isinstance(dt, timedelta):
                    json_data[i] = dt
                else:
                    raise RuntimeError(f"Unknown type for {dt} key {i}")
            elif isinstance(event[i], icalendar.prop.vText):
                json_data[i] = event[i]
            elif isinstance(event[i], int):
                json_data[i] = event[i]
            elif isinstance(event[i], str):
                json_data[i] = event[i]
            else:
                raise RuntimeError(f"Unknown type for {i}")

        result.append(json_data)
    return result


if __name__ == "__main__":
    ical_string = urllib.request.urlopen("http://tinyurl.com/y24m3r8f").read()
    print(read_calendar(ical_string, date(2019, 3, 5), date(2019, 4, 1)))
