#![cfg(feature = "daemon")]

use paper::cli::daemon::{render_launchd_plist, render_systemd_unit};
use std::path::Path;

#[test]
fn systemd_unit_contains_executable_and_service_settings() {
    let exe = Path::new("/opt/Paper Suite/paper");
    let unit = render_systemd_unit(exe);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paper.service");
    std::fs::write(&path, &unit).unwrap();
    let written = std::fs::read_to_string(path).unwrap();

    assert!(written.contains("[Service]"));
    assert!(written.contains("ExecStart=\"/opt/Paper Suite/paper\" daemon --foreground"));
    assert!(written.contains("Restart=on-failure"));
    assert!(written.contains("[Install]"));
    assert!(written.contains("WantedBy=default.target"));
}

#[test]
fn launchd_plist_contains_executable_arguments_and_log_paths() {
    let exe = Path::new("/Applications/Paper Suite/paper");
    let cache = Path::new("/tmp/paper cache");
    let plist = render_launchd_plist(exe, cache);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tools.kintsugi.paper.plist");
    std::fs::write(&path, &plist).unwrap();
    let written = std::fs::read_to_string(path).unwrap();

    assert!(written.contains("<key>Label</key>"));
    assert!(written.contains("<string>tools.kintsugi.paper</string>"));
    assert!(written.contains("<key>ProgramArguments</key>"));
    assert!(written.contains("<string>/Applications/Paper Suite/paper</string>"));
    assert!(written.contains("<string>daemon</string>"));
    assert!(written.contains("<string>--foreground</string>"));
    assert!(written.contains("<key>RunAtLoad</key>\n    <true/>"));
    assert!(written.contains("<key>KeepAlive</key>\n    <false/>"));
    assert!(written.contains("<string>/tmp/paper cache/daemon.stdout.log</string>"));
    assert!(written.contains("<string>/tmp/paper cache/daemon.stderr.log</string>"));
}

#[cfg(unix)]
mod unix {
    use paper::daemon::ipc::{run_ipc_server, DaemonRequest, DaemonResponse, IpcClient};
    use paper::daemon::DaemonHandler;
    use std::path::PathBuf;
    use std::process::{Child, Command, Output};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tempfile::TempDir;
    use tokio::net::UnixStream;
    use tokio::task::JoinHandle;

    #[derive(Default)]
    struct State {
        paused: bool,
    }

    struct TestHandler {
        state: Arc<Mutex<State>>,
    }

    #[async_trait::async_trait]
    impl DaemonHandler for TestHandler {
        async fn handle(&self, request: DaemonRequest) -> DaemonResponse {
            match request {
                DaemonRequest::GetStatus => {
                    let state = self.state.lock().unwrap();
                    DaemonResponse::Status {
                        indexed_files: 42,
                        queued: 3,
                        paused: state.paused,
                        model: Some("test-model".to_string()),
                    }
                }
                DaemonRequest::Pause => {
                    self.state.lock().unwrap().paused = true;
                    DaemonResponse::Ok
                }
                DaemonRequest::Resume => {
                    self.state.lock().unwrap().paused = false;
                    DaemonResponse::Ok
                }
                DaemonRequest::IndexRoots { .. } => DaemonResponse::IndexRoots {
                    stats: paper::core::index::IndexStats::default(),
                },
                DaemonRequest::AddRoot { .. }
                | DaemonRequest::RemoveRoot { .. }
                | DaemonRequest::Shutdown => DaemonResponse::Ok,
            }
        }
    }

    struct TestDaemon {
        dir: TempDir,
        socket: PathBuf,
        server: JoinHandle<()>,
        state: Arc<Mutex<State>>,
    }

    impl TestDaemon {
        async fn start() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let socket = dir.path().join("paper.sock");
            let state = Arc::new(Mutex::new(State::default()));
            let server_socket = socket.clone();
            let handler: Arc<dyn DaemonHandler> = Arc::new(TestHandler {
                state: state.clone(),
            });
            let server = tokio::spawn(async move {
                let _ = run_ipc_server(&server_socket, handler).await;
            });

            let mut ready = false;
            for _ in 0..50 {
                if UnixStream::connect(&socket).await.is_ok() {
                    ready = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(ready, "test IPC server did not start");

            Self {
                dir,
                socket,
                server,
                state,
            }
        }

        fn control(&self, args: &[&str]) -> Output {
            Command::new(env!("CARGO_BIN_EXE_paper"))
                .env("PAPER_HOME", self.dir.path())
                .arg("daemon-control")
                .args(args)
                .arg("--socket")
                .arg(&self.socket)
                .output()
                .unwrap()
        }
    }

    impl Drop for TestDaemon {
        fn drop(&mut self) {
            self.server.abort();
        }
    }

    #[tokio::test]
    async fn ipc_client_round_trips_status_pause_and_resume() {
        let daemon = TestDaemon::start().await;
        let mut client = IpcClient::connect(&daemon.socket).await.unwrap();

        let response = client.send(DaemonRequest::GetStatus).await.unwrap();
        assert!(matches!(
            response,
            DaemonResponse::Status {
                indexed_files: 42,
                queued: 3,
                paused: false,
                ..
            }
        ));

        assert!(matches!(
            client.send(DaemonRequest::Pause).await.unwrap(),
            DaemonResponse::Ok
        ));
        assert!(daemon.state.lock().unwrap().paused);
        assert!(matches!(
            client.send(DaemonRequest::Resume).await.unwrap(),
            DaemonResponse::Ok
        ));
        assert!(!daemon.state.lock().unwrap().paused);
    }

    #[test]
    fn status_command_supports_human_and_json_output() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let daemon = TestDaemon::start().await;

            let human = daemon.control(&["status"]);
            assert!(human.status.success(), "{human:?}");
            let stdout = String::from_utf8_lossy(&human.stdout);
            assert!(stdout.contains("42"), "{stdout}");
            assert!(stdout.contains("3"), "{stdout}");
            assert!(stdout.contains("false"), "{stdout}");
            assert!(stdout.contains("test-model"), "{stdout}");

            let json = daemon.control(&["status", "--json"]);
            assert!(json.status.success(), "{json:?}");
            let value: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
            assert_eq!(value["type"], "status");
            assert_eq!(value["indexed_files"], 42);
            assert_eq!(value["queued"], 3);
            assert_eq!(value["paused"], false);
            assert_eq!(value["model"], "test-model");
        });
    }

    #[test]
    fn pause_and_resume_commands_print_confirmations() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let daemon = TestDaemon::start().await;

            let pause = daemon.control(&["pause"]);
            assert!(pause.status.success(), "{pause:?}");
            assert!(String::from_utf8_lossy(&pause.stdout).contains("paused"));
            assert!(daemon.state.lock().unwrap().paused);

            let resume = daemon.control(&["resume"]);
            assert!(resume.status.success(), "{resume:?}");
            assert!(String::from_utf8_lossy(&resume.stdout).contains("resumed"));
            assert!(!daemon.state.lock().unwrap().paused);
        });
    }

    struct ChildGuard(Child);

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn stop_command_shuts_down_daemon_process() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("daemon.sock");
        let docs = dir.path().join("docs");
        std::fs::create_dir(&docs).unwrap();
        let child = Command::new(env!("CARGO_BIN_EXE_paper"))
            .env("PAPER_HOME", dir.path())
            .arg("daemon")
            .arg("--foreground")
            .arg("--socket")
            .arg(&socket)
            .arg("--root")
            .arg(&docs)
            .arg("--no-watch")
            .arg("--no-schedule")
            .arg("--no-resource-guard")
            .spawn()
            .unwrap();
        let mut child = ChildGuard(child);

        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut ready = false;
            for _ in 0..50 {
                if IpcClient::connect(&socket).await.is_ok() {
                    ready = true;
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(ready, "daemon process did not open its IPC socket");
        });

        let output = Command::new(env!("CARGO_BIN_EXE_paper"))
            .env("PAPER_HOME", dir.path())
            .arg("daemon-control")
            .arg("stop")
            .arg("--socket")
            .arg(&socket)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");

        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "daemon exited with {status}");
                break;
            }
            assert!(Instant::now() < deadline, "daemon ignored Shutdown");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn missing_daemon_error_explains_how_to_start() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("missing.sock");
        let output = Command::new(env!("CARGO_BIN_EXE_paper"))
            .env("PAPER_HOME", dir.path())
            .arg("daemon-control")
            .arg("status")
            .arg("--socket")
            .arg(socket)
            .output()
            .unwrap();

        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("paper daemon --foreground"), "{stderr}");
        assert!(stderr.contains("paper daemon-control install"), "{stderr}");
    }
}
