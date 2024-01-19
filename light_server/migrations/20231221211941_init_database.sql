
CREATE TABLE IF NOT EXISTS event_action_groups
(
		event_type  TEXT NOT NULL,
		device_group_id  INTEGER NOT NULL,
		commands  TEXT NOT NULL,
);

-- Device 
CREATE TABLE IF NOT EXISTS device_groups
(
    id          INTEGER PRIMARY KEY NOT NULL,
    description TEXT                NOT NULL,
    name        TEXT                NOT NULL,
);

CREATE TABLE IF NOT EXISTS device_group_members
(
    device_group_id INTEGER NOT NULL,
    device_id       INTEGER NOT NULL,
    FOREIGN KEY (device_group_id) REFERENCES device_groups (id),
    FOREIGN KEY (device_id) REFERENCES devices (id)
);

CREATE TABLE IF NOT EXISTS devices
(
    id          INTEGER PRIMARY KEY NOT NULL,
    description TEXT                NOT NULL,
    name        TEXT                NOT NULL,
);
-- Add migration script here
