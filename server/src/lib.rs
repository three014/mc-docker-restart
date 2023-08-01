use shared::{Action, Command, STOP_CODE};
use std::process::{Output, Stdio};
use tokio::{
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{tcp::OwnedWriteHalf, TcpListener, TcpStream, ToSocketAddrs},
    process::{Child, Command as Process},
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

static BIND_ADDR: &str = "127.0.0.1:4086";
static SERVER_NAME: &str = "lads-mc";

pub struct McRemoteServer {
    listener: TcpListener,
    tasks: TaskDb,
}

struct TaskDb {
    tasks: Vec<Option<TaskHandle>>,
    cur_id: u64,
}

impl TaskDb {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            cur_id: 0,
        }
    }

    fn next_id(&mut self) -> u64 {
        let next_id = self.cur_id;
        self.cur_id += 1;
        next_id
    }

    pub fn register(&mut self, ctrl_c: mpsc::Sender<u64>) -> Task {
        Task::new(self.next_id(), ctrl_c)
    }

    pub fn store(&mut self, task_handle: TaskHandle) {
        let id = *&task_handle.id as usize;
        self.tasks.resize_with(id + 1, Default::default);
        self.tasks[id] = Some(task_handle);
    }

    pub fn delete(&mut self, id: u64) {
        if { id as usize } < self.tasks.len() {
            if let Some(task) = self.tasks[id as usize].take() {
                tokio::spawn(async move {
                    task.handle.abort();
                    let _ = task.handle.await;
                    if let Ok(mut proc) = task.maybe_proc.await {
                        let _ = proc.kill().await;
                        eprintln!("Killed child process");
                    }
                });
            }
        }
    }
}

impl McRemoteServer {
    pub async fn bind<A>(addr: A) -> io::Result<Self>
    where
        A: ToSocketAddrs,
    {
        Ok(Self {
            listener: TcpListener::bind(addr).await?,
            tasks: TaskDb::new(),
        })
    }

    pub async fn serve(mut self) -> io::Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u64>(15);
        loop {
            tokio::select! {
                Some(id) = rx.recv() => {
                    eprintln!("Received command to kill {id}");
                    self.tasks.delete(id);
                },
                result = self.listener.accept() => {
                    let (socket, _) = result?;
                    eprintln!("New connection!");
                    self.handle_connection(socket, tx.clone()).await?;
                }
            };
        }
    }

    async fn handle_connection(
        &mut self,
        mut socket: TcpStream,
        ctrl_c: mpsc::Sender<u64>,
    ) -> tokio::io::Result<()> {
        match Command::try_from_reader(&mut socket).await {
            Ok(command) => {
                let task = self.tasks.register(ctrl_c);
                self.tasks.store(task.run(command, socket));
                Ok(())
            }
            Err(err) => socket.write_all(err.as_bytes()).await,
        }
    }
}

struct TaskHandle {
    id: u64,
    handle: JoinHandle<io::Result<()>>,
    maybe_proc: oneshot::Receiver<Child>,
}

struct Task {
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
                        let mut buf = Vec::with_capacity(1024);
                        let mut client_buf = [0u8; 1];

                        loop {
                            buf.clear();
                            tokio::select! {
                                result = reader.read_until(0xA, &mut buf) => {
                                    let _ = result?;
                                    let bytes_sent = tx.write(&buf).await?;
                                    if bytes_sent == 0 {
                                        return Ok(());
                                    } else {
                                        eprintln!("Sent {bytes_sent} bytes to client!");
                                    }
                                }
                                result = rx.read(&mut client_buf) => {
                                    eprintln!("Received message from client!");
                                    let byte_read = result?;
                                    if byte_read == 1 && client_buf[0] == STOP_CODE {
                                        eprintln!("Client said to stop please");
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

pub async fn launch() -> tokio::io::Result<()> {
    let listener = TcpListener::bind(BIND_ADDR).await?;
    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(handle_connection(socket));
    }
}

async fn handle_connection(socket: TcpStream) -> tokio::io::Result<()> {
    let (mut rx, mut tx) = socket.into_split();
    match Command::try_from_reader(&mut rx).await {
        Ok(command) => run(command, tx).await,
        Err(err) => tx.write_all(err.as_bytes()).await,
    }
}

async fn run(command: Command, mut tx: OwnedWriteHalf) -> tokio::io::Result<()> {
    match command.get() {
        Action::Logs(log_args) => {
            let args = ["logs", SERVER_NAME, "-f"];
            if log_args.follow() {
                let mut process = docker_follow(&args).await?;
                let possible_err = io::Error::last_os_error();
                let stdout = process.stdout.take().ok_or(possible_err)?;
                let mut reader = BufReader::new(stdout);
                let mut buf = String::with_capacity(1024);

                loop {
                    buf.clear();
                    reader.read_line(&mut buf).await?;
                    let bytes_sent = tx.write(buf.as_bytes()).await?;
                    if bytes_sent == 0 {
                        break; // Client tcpstream closed
                    } else {
                        eprintln!("Sent {bytes_sent} bytes to client!");
                    }
                }

                process.kill().await
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
}

async fn docker(args: &[&str]) -> io::Result<Output> {
    Process::new("docker").args(args).output().await
}

async fn docker_follow(args: &[&str]) -> io::Result<Child> {
    Process::new("docker")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
}
