CREATE TABLE IF NOT EXISTS events
(
    id          INTEGER PRIMARY KEY NOT NULL,
    description TEXT                NOT NULL,
    message_id  TEXT             NOT NULL DEFAULT 0
);
