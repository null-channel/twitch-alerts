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
    assert!(!help.contains("--openai-key"));
    let output = Command::new(env!("CARGO_BIN_EXE_bruce"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .args(["server", "--help"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}
