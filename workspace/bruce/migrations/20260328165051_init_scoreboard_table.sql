-- Add migration script here
-- id | username | platform_youtube_id | platform_twitch_id | platform | totalpoints
CREATE TABLE wordle_highscores
(
    id INTEGER PRIMARY KEY,
    username TEXT NOT NULL,
    platform_user_id TEXT NOT NULL,
    platform INTEGER NOT NULL,
    total_points INTEGER NOT NULL
);

CREATE INDEX wordle_highscores_platfrom_id ON wordle_highscores (platform,platform_user_id);
