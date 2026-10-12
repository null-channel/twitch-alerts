use std::process::Command;

#[test]
fn game_cli_runs_outside_the_source_tree() {
    let output = Command::new(env!("CARGO_BIN_EXE_bruce"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .args(["game", "wordle", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--word-list"));
    assert!(help.contains("--db-path"));
    assert!(help.contains("--tc-username"));
    assert!(help.contains("--yt-api-key"));
    assert!(help.contains("--yt-video-id"));
    assert!(help.contains("--yt-channel-id"));
    assert!(!help.contains("--openai-key"));
    let output = Command::new(env!("CARGO_BIN_EXE_bruce"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .args(["server", "--help"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn game_refuses_to_start_without_a_chat_platform() {
    let output = Command::new(env!("CARGO_BIN_EXE_bruce"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .env("BRUCE_LOG", std::env::temp_dir().join("bruce-cli-test.log"))
        .args(["game", "wordle"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no chat platform configured"), "{}", stderr);
    assert!(stderr.contains("YT_API_KEY"), "{}", stderr);
}

#[test]
fn partially_configured_platforms_are_rejected_before_connecting() {
    let output = Command::new(env!("CARGO_BIN_EXE_bruce"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .env("BRUCE_LOG", std::env::temp_dir().join("bruce-cli-test.log"))
        .env("YT_API_KEY", "key-without-a-broadcast")
        .args(["game", "wordle"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("YT_VIDEO_ID"), "{}", stderr);
    assert!(!stderr.contains("key-without-a-broadcast"), "{}", stderr);
}
