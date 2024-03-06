-- Add migration script here
create schema "tower_sessions";

create table "tower_sessions"."session" (
    id text primary key not null,
    data bytea not null,
    expiry_date timestamptz not null
);