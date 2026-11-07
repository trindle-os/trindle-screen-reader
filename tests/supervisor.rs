//! The start, status and stop commands against the real binary, in a private runtime folder.

use std::process::Command;

fn reader(dir: &std::path::Path, arg: &str) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_trindle-screen-reader"))
        .arg(arg)
        .env("XDG_RUNTIME_DIR", dir)
        .env("DBUS_SESSION_BUS_ADDRESS", "unix:path=/nonexistent/bus")
        .env("AT_SPI_BUS_ADDRESS", "unix:path=/nonexistent/at-spi")
        .output()
        .unwrap()
}

#[test]
fn start_status_and_stop_tell_the_truth() {
    let dir = std::env::temp_dir().join(format!("trindle-sr-sup-it-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
    assert_eq!(reader(&dir, "status").stdout, b"not running\n");
    assert!(reader(&dir, "start").status.success());
    assert_eq!(reader(&dir, "status").stdout, b"running\n");
    assert!(reader(&dir, "start").status.success(), "starting twice is fine");
    let pid = std::fs::read_to_string(dir.join("trindle-screen-reader/supervisor.pid")).unwrap();
    assert!(reader(&dir, "stop").status.success());
    assert_eq!(reader(&dir, "status").stdout, b"not running\n");
    assert!(!std::path::Path::new(&format!("/proc/{}", pid.trim())).exists(), "the watcher is gone");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn without_a_runtime_folder_start_refuses_instead_of_using_tmp() {
    let out = Command::new(env!("CARGO_BIN_EXE_trindle-screen-reader")).arg("start").env_remove("XDG_RUNTIME_DIR").output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("runtime folder"));
}
