CREATE TABLE IF NOT EXISTS sub_events
(
    user_id       INTEGER PRIMARY KEY NOT NULL,
    user_name     TEXT                NOT NULL,
    subscribed_at DATETIME            NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS event_queue
(
    message_id    INTEGER PRIMARY KEY NOT NULL,
    event_data    TEXT                NOT NULL,
    event_at      DATETIME            NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_processed  BOOLEAN             NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS follow_events
(
    user_id       INTEGER PRIMARY KEY NOT NULL,
    user_name     TEXT                NOT NULL,
    tier          INTEGER             NOT NULL DEFAULT 1000,
    is_gift       BOOLEAN             NOT NULL DEFAULT 0,
    followed_at   DATETIME            NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS story_segments
(
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id       INTEGER             NOT NULL, 
    event_type    TEXT                NOT NULL,
    story_segment TEXT                NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS raid_events
(
    user_id       INTEGER PRIMARY KEY NOT NULL,
    user_name     TEXT                NOT NULL,
    raiders       INTEGER             NOT NULL DEFAULT 0,
    raid_at       DATETIME            NOT NULL DEFAULT CURRENT_TIMESTAMP,
    story_segment TEXT                NOT NULL DEFAULT 0
);