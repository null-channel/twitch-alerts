use std::process::Command;

#[test]
fn alerts_cli_runs_outside_the_source_tree() {
    let output = Command::new(env!("CARGO_BIN_EXE_twitch_alerts"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .args(["server", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--openai-key"));
    assert!(help.contains("--frontend-assets"));
    assert!(!help.contains("--word-list"));
    let output = Command::new(env!("CARGO_BIN_EXE_twitch_alerts"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .args(["game", "--help"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
