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

#[test]
fn authorization_command_is_independent_of_the_alerts_server() {
    let output = Command::new(env!("CARGO_BIN_EXE_twitch_alerts"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .env("TWITCH_CLIENT_SECRET", "do-not-print-this-secret")
        .args(["authorize", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    assert!(help.contains("--twitch-client-id"));
    assert!(help.contains("--token-file"));
    assert!(help.contains("--redirect-url"));
    assert!(!help.contains("--openai-key"));
    assert!(!help.contains("do-not-print-this-secret"));

    let output = Command::new(env!("CARGO_BIN_EXE_twitch_alerts"))
        .current_dir(std::env::temp_dir())
        .env_clear()
        .args([
            "server",
            "--token-file",
            "/missing-twitch-alerts-test-directory/tokens.json",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("run `twitch_alerts authorize` once first"));
    assert!(!error.contains("required arguments"));
}
