use fatir::{auth_watcher, continuations, observer, ollama, proactive};
use std::{fs, path::PathBuf};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::UnixListener};

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join(".local/share"))
        .join("Fatir")
}

fn socket_path() -> PathBuf { data_dir().join("core.sock") }

async fn bind_singleton() -> anyhow::Result<UnixListener> {
    fs::create_dir_all(data_dir())?;
    let path=socket_path();
    if path.exists() {
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            anyhow::bail!("FATIR_CORE_ALREADY_RUNNING");
        }
        let _=fs::remove_file(&path);
    }
    Ok(UnixListener::bind(path)?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let listener=match bind_singleton().await {
        Ok(x)=>x,
        Err(e) if e.to_string()=="FATIR_CORE_ALREADY_RUNNING" => return Ok(()),
        Err(e)=>return Err(e),
    };

    let state=ollama::new_state();

    tokio::spawn(observer::run_forever());
    tokio::spawn(proactive::monitor_forever());
    tokio::spawn(continuations::monitor_forever(state.clone()));
    tokio::spawn(auth_watcher::monitor_forever(state.clone()));

    loop {
        let Ok((mut stream,_))=listener.accept().await else { continue; };
        tokio::spawn(async move {
            let mut buf=[0u8;128];
            let n=stream.read(&mut buf).await.unwrap_or(0);
            let cmd=String::from_utf8_lossy(&buf[..n]).trim().to_ascii_uppercase();
            let reply=if cmd=="PING" {"PONG\n"} else {"OK\n"};
            let _=stream.write_all(reply.as_bytes()).await;
        });
    }
}
