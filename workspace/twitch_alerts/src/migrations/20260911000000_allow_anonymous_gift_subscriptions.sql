-- Twitch omits the gifter and cumulative count for anonymous gifts.
CREATE TABLE gift_subs_events_updated
(
    id                  INTEGER PRIMARY KEY NOT NULL,
    broadcaster_user_id TEXT                NOT NULL,
    cumulative_total    INTEGER,
    is_anonymous        BOOLEAN             NOT NULL,
    tier                TEXT                NOT NULL,
    total               INTEGER             NOT NULL,
    user_id             TEXT,
    user_login          TEXT,
    user_name           TEXT,
    story_segment       TEXT                NOT NULL
);

INSERT INTO gift_subs_events_updated (
    id, broadcaster_user_id, cumulative_total, is_anonymous, tier,
    total, user_id, user_login, user_name, story_segment
)
SELECT id, broadcaster_user_id, cumulative_total, is_anonymous, tier,
       total, user_id, user_login, user_name, story_segment
FROM gift_subs_events;

DROP TABLE gift_subs_events;
ALTER TABLE gift_subs_events_updated RENAME TO gift_subs_events;
