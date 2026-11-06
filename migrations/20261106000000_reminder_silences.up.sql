create table if not exists reminder_silences (
    id bigserial primary key,
    reminder_key text not null,
    expires_at timestamptz,
    reason text,
    created_at timestamptz not null default now()
);

create index if not exists idx_reminder_silences_key on reminder_silences(reminder_key);
create index if not exists idx_reminder_silences_expires_at on reminder_silences(expires_at);
