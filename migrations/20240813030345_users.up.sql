create table if not exists users (
    id SERIAL PRIMARY KEY,
    name text not null,
    oidc_id text not null unique,
    email text not null,
    is_admin boolean not null default false
);
create table groups(
    id SERIAL PRIMARY KEY,
    name text not null unique
);
create table if not exists user_groups (
    id SERIAL PRIMARY KEY,
    user_id integer not null,
    group_id integer not null,
    foreign key (user_id) references users(id) on update cascade on delete cascade,
    foreign key (group_id) references groups(id) on update cascade on delete cascade
);