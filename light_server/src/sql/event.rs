
struct Event {
    event_type: String,
    device_group_id: u32,
    commands: Vec<Command>,
} 

struct Command {
    command: CommandType,
    //TODO: add command parameters
    duration: u32,
    red: u8,
    green: u8,
    blue: u8,
}

enum CommandType {
    RAINBOW_CYCLE,
    THEATER_CHASE,
    THEATER_CHASE_RAINBOW,
    COLOR_WIPE,
    BLINK,
    FADE_COLOR,
    SPARKLE,
    SPARKLE_FADE,
    BREATH,
    SOLID_SNAKE,
    //Server side commands
    RANDOM,
}

struct Color {
    red: u8,
    green: u8,
    blue: u8,
}


struct DeviceGroup {
    id: u32,
    name: String,
    devices: Vec<u32>,
}

struct Device {
    id: u32,
    name: String,
    description: String,
    default_command: Command,
}




/*
* CREATE TABLE IF NOT EXISTS event_action_groups
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
);
-- Add migration script here
*/
