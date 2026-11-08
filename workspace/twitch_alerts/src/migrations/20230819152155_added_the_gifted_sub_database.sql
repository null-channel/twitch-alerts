-- Add migration script here

CREATE TABLE IF NOT EXISTS gift_subs_events
(
    id                          INTEGER PRIMARY KEY NOT NULL,
    broadcaster_user_id         TEXT                NOT NULL,
    cumulative_total            INTEGER             NOT NULL,
    is_anonymous                BOOLEAN             NOT NULL,
    tier                        INTEGER             NOT NULL,
    total                       INTEGER             NOT NULL,
    user_id                     TEXT                NOT NULL,
    user_login                  TEXT                NOT NULL,
    user_name                   TEXT                NOT NULL,
    story_segment               TEXT                NOT NULL
);