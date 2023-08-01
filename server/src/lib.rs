use shared::Command;
use tokio::{
    io::{self, AsyncWriteExt},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    sync::mpsc,
};

static SERVER_NAME: &str = "lads-mc";

pub struct McRemoteServer {
    listener: TcpListener,
    tasks: db::TaskDb,
}

mod db {
    use tokio::sync::mpsc;

    mod task;

    pub struct TaskDb {
        tasks: Vec<Option<task::TaskHandle>>,
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

        pub fn register(&mut self, ctrl_c: mpsc::Sender<u64>) -> task::Task {
            let id = self.next_id();
            log::debug!("Registered task with id: {id}");
            task::Task::new(id, ctrl_c)
        }

        pub fn store(&mut self, task_handle: task::TaskHandle) {
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
                            let id = proc.id();
                            log::debug!("Killing child process {:?}", id);
                            let _ = proc.kill().await;
                            log::debug!("Killed child process {:?}", id);
                        }
                    });
                }
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
            tasks: db::TaskDb::new(),
        })
    }

    pub async fn serve(mut self) -> io::Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<u64>(15);
        loop {
            tokio::select! {
                Some(id) = rx.recv() => {
                    log::debug!("Received command to kill task with id: {id}");
                    self.tasks.delete(id);
                },
                result = self.listener.accept() => {
                    let (socket, _) = result?;
                    log::info!("New connection!");
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
                Ok(self.tasks.store(task.run(command, socket)))
            }
            Err(err) => socket.write_all(err.as_bytes()).await,
        }
    }
}
