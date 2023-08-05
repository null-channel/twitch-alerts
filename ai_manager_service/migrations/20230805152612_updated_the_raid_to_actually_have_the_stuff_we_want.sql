-- Add migration script here

DROP TABLE IF EXISTS raid_events;

CREATE TABLE raid_events
(
    id                          INTEGER PRIMARY KEY NOT NULL,
    from_broadcaster_user_id    TEXT                NOT NULL,
    from_broadcaster_user_name  TEXT                NOT NULL,
    to_broadcaster_user_id      TEXT                NOT NULL,
    to_broadcaster_user_name    TEXT                NOT NULL,
    viewers                     INTEGER             NOT NULL,
    story_segment               TEXT                NOT NULL,
    raid_at                     DATETIME            NOT NULL DEFAULT CURRENT_TIMESTAMP
);
