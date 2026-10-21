// This file is part of Clippy.
//
// Copyright (c) 2026  René Coignard <contact@renecoignard.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
use std::cell::Cell;
use std::ptr::NonNull;

use block2::RcBlock;
use clippy::calendar::{Agenda, Appointment};
use dispatch2::DispatchQueue;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{SignedDuration, Timestamp, ToSpan, Zoned};
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2::{MainThreadMarker, available};
use objc2_event_kit::{
    EKAuthorizationStatus, EKEntityType, EKEvent, EKEventStatus, EKEventStore,
    EKEventStoreChangedNotification, EKParticipantStatus,
};
use objc2_foundation::{
    NSBundle, NSCalendarDayChangedNotification, NSDate, NSError, NSNotification,
    NSNotificationCenter, NSOperationQueue, ns_string,
};

use crate::delegate::Delegate;

const DAYS_AHEAD: i64 = 8;

pub(crate) struct Calendars {
    store: Retained<EKEventStore>,
    asked: Cell<bool>,
    granted: Cell<bool>,
}

impl Calendars {
    pub(crate) fn new() -> Self {
        Self {
            store: unsafe { EKEventStore::new() },
            asked: Cell::new(false),
            granted: Cell::new(false),
        }
    }

    pub(crate) fn watch(&self) {
        let center = NSNotificationCenter::defaultCenter();
        let main = NSOperationQueue::mainQueue();
        let reread = RcBlock::new(|_: NonNull<NSNotification>| reread());
        unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(EKEventStoreChangedNotification),
                Some(&self.store),
                Some(&main),
                &reread,
            );
            center.addObserverForName_object_queue_usingBlock(
                Some(NSCalendarDayChangedNotification),
                None,
                Some(&main),
                &reread,
            );
        }
    }

    pub(crate) fn agenda(&self, now: &Zoned) -> Option<Agenda> {
        match unsafe { EKEventStore::authorizationStatusForEntityType(EKEntityType::Event) } {
            EKAuthorizationStatus::FullAccess => {
                if !self.granted.replace(true) {
                    unsafe { self.store.reset() };
                }
                Some(Agenda::Read(self.appointments(now)))
            }
            EKAuthorizationStatus::NotDetermined => {
                let may_ask = may_ask();
                if !self.asked.replace(true) {
                    if may_ask {
                        self.ask();
                    } else {
                        log::warn!(
                            "Clippy can only ask to see your calendars when it runs as Clippy.app"
                        );
                    }
                }
                (!may_ask).then_some(Agenda::Denied)
            }
            _ => Some(Agenda::Denied),
        }
    }

    fn ask(&self) {
        let answered = RcBlock::new(|_granted: Bool, _error: *mut NSError| {
            DispatchQueue::main().exec_async(reread);
        });
        let answered = RcBlock::as_ptr(&answered);
        if available!(macos = 14.0) {
            unsafe { self.store.requestFullAccessToEventsWithCompletion(answered) };
        } else {
            #[expect(deprecated)]
            unsafe {
                self.store
                    .requestAccessToEntityType_completion(EKEntityType::Event, answered);
            }
        }
    }

    fn appointments(&self, now: &Zoned) -> Vec<Appointment> {
        let Some((from, until)) = now.start_of_day().ok().and_then(|from| {
            Some((
                from.timestamp(),
                from.checked_add(DAYS_AHEAD.days()).ok()?.timestamp(),
            ))
        }) else {
            return Vec::new();
        };
        let events = unsafe {
            let predicate = self
                .store
                .predicateForEventsWithStartDate_endDate_calendars(&date(from), &date(until), None);
            self.store.eventsMatchingPredicate(&predicate)
        };
        events
            .iter()
            .filter_map(|event| appointment(&event, now.time_zone()))
            .collect()
    }
}

fn reread() {
    let mtm = MainThreadMarker::new().expect("Clippy should reread calendars on the main thread");
    if let Some(delegate) = Delegate::shared(mtm) {
        delegate.read_calendars();
    }
}

fn may_ask() -> bool {
    let key = if available!(macos = 14.0) {
        ns_string!("NSCalendarsFullAccessUsageDescription")
    } else {
        ns_string!("NSCalendarsUsageDescription")
    };
    NSBundle::mainBundle()
        .objectForInfoDictionaryKey(key)
        .is_some()
}

fn appointment(event: &EKEvent, zone: &TimeZone) -> Option<Appointment> {
    unsafe {
        if event.status() == EKEventStatus::Canceled || declined(event) {
            return None;
        }
        let occurrence = event.occurrenceDate().unwrap_or_else(|| event.startDate());
        Some(Appointment {
            id: format!(
                "{}@{:.0}",
                event.eventIdentifier()?,
                occurrence.timeIntervalSince1970()
            ),
            calendar: event
                .calendar()
                .map(|calendar| calendar.title().to_string())
                .unwrap_or_default(),
            title: event.title().to_string(),
            start: civil(&event.startDate(), zone)?,
            end: civil(&event.endDate(), zone)?,
            all_day: event.isAllDay(),
        })
    }
}

fn declined(event: &EKEvent) -> bool {
    unsafe { event.attendees() }.is_some_and(|attendees| {
        attendees.iter().any(|attendee| unsafe {
            attendee.isCurrentUser()
                && attendee.participantStatus() == EKParticipantStatus::Declined
        })
    })
}

fn civil(date: &NSDate, zone: &TimeZone) -> Option<DateTime> {
    let since_epoch = SignedDuration::try_from_secs_f64(date.timeIntervalSince1970()).ok()?;
    let timestamp = Timestamp::from_duration(since_epoch).ok()?;
    Some(zone.to_datetime(timestamp))
}

fn date(timestamp: Timestamp) -> Retained<NSDate> {
    NSDate::dateWithTimeIntervalSince1970(timestamp.as_duration().as_secs_f64())
}
