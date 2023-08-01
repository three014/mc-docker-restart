use crate::SERVER_NAME;
    use shared::{Action, Command, STOP_CODE};
    use std::process::{Output, Stdio};
    use tokio::{
        io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::TcpStream,
        process::{Child, Command as Process},
        sync::{mpsc, oneshot},
        task::JoinHandle,
    };

    pub struct TaskHandle {
        pub id: u64,
        pub handle: JoinHandle<io::Result<()>>,
        pub maybe_proc: oneshot::Receiver<Child>,
    }

    pub struct Task {
        id: u64,
        ctrl_c: mpsc::Sender<u64>,
    }

    impl Task {
        pub fn new(id: u64, ctrl_c: mpsc::Sender<u64>) -> Self {
            Self { id, ctrl_c }
        }

        pub fn run(self, command: Command, mut stream: TcpStream) -> TaskHandle {
            // Create proc sender/receiver
            let (proc, recv) = oneshot::channel();
            let id = self.id;
            let ctrl_c = self.ctrl_c;
            let handle = tokio::spawn(async move {
                // Split stream into read and write
                let (mut rx, mut tx) = stream.split();
                match command.get() {
                    Action::Logs(log_args) => {
                        let args = ["logs", SERVER_NAME, "-f"];
                        if log_args.follow() {
                            let mut process = docker_follow(&args).await?;
                            let possible_err = io::Error::last_os_error();
                            let stdout = process.stdout.take().ok_or(possible_err)?;
                            let _ = proc.send(process);

                            let mut reader = BufReader::new(stdout);
                            let mut proc_buf = Vec::with_capacity(2048);
                            let mut client_buf = [0u8; 1];

                            loop {
                                proc_buf.clear();
                                tokio::select! {
                                    result = reader.read_until(0xA, &mut proc_buf) => {
                                        let _ = result?;
                                        let bytes_sent = tx.write(&proc_buf).await?;
                                        if bytes_sent == 0 {
                                            return Ok(());
                                        } else {
                                            log::trace!("Sent {bytes_sent} bytes to client!");
                                        }
                                    }
                                    result = rx.read(&mut client_buf) => {
                                        let byte_read = result?;
                                        if byte_read == 1 && client_buf[0] == STOP_CODE {
                                            log::info!("Client signaled to disconnect");
                                            let _ = ctrl_c.send(id).await;
                                            break;
                                        }
                                    }
                                }
                            }
                            Ok(())
                        } else {
                            let output = docker(&args[..2]).await?;
                            tx.write_all(&output.stdout).await
                        }
                    }
                    Action::Start => {
                        let output = docker(&["compose", "up", SERVER_NAME, "-d"]).await?;
                        tx.write_all(&output.stderr).await
                    }
                    Action::Stop => {
                        let output = docker(&["compose", "stop", SERVER_NAME]).await?;
                        tx.write_all(&output.stderr).await
                    }
                    Action::Restart => {
                        let output = docker(&["compose", "restart", SERVER_NAME]).await?;
                        tx.write_all(&output.stderr).await
                    }
                }
            });

            TaskHandle {
                id,
                handle,
                maybe_proc: recv,
            }
        }
    }

    pub(crate) async fn docker(args: &[&str]) -> io::Result<Output> {
        log::info!("Spawning child process and collecting output...");
        Process::new("docker").args(args).output().await
    }

    pub(crate) async fn docker_follow(args: &[&str]) -> io::Result<Child> {
        let child = Process::new("docker")
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        log::info!("Spawned child process with id: {:?}", child.id());
        Ok(child)
    }
