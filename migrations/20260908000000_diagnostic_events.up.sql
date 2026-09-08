create table if not exists diagnostic_events (
    id SERIAL PRIMARY KEY,
    created_at timestamptz not null default now(),
    device_id text not null,
    event_type text not null,
    payload jsonb not null
);

create index if not exists idx_diagnostic_events_created_at on diagnostic_events(created_at);
create index if not exists idx_diagnostic_events_device_id on diagnostic_events(device_id);
create index if not exists idx_diagnostic_events_event_type on diagnostic_events(event_type);
