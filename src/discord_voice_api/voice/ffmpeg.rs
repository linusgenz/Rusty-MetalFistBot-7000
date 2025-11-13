use anyhow::Result;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::{io::AsyncWriteExt, process::Command as TokioCommand};

pub async fn spawn_ffmpeg_with_buffer(
    url: &str,
    buffer_size: usize,
) -> Result<(tokio::process::Child, tokio::process::ChildStdout)> {
    let args = [
        "-i", "pipe:0",
        "-f", "s16le",
        "-ar", "48000",
        "-ac", "2",
        "pipe:1"
    ];

    let mut child = TokioCommand::new("ffmpeg")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let mut ffmpeg_stdin = child.stdin.take().expect("child stdin");
    let ffmpeg_stdout = child.stdout.take().expect("child stdout");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(buffer_size);
    let url_owned = url.to_string();

    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let total_size = match client.head(&url_owned).send().await {
            Ok(resp) => resp
                .headers()
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0),
            Err(_) => 0,
        };

        println!("[FETCHER] Total size: {} bytes", total_size);

        let mut start: u64 = 0;
        let chunk_size: u64 = 256 * 1024;

        while start < total_size {
            let end = (start + chunk_size - 1).min(total_size - 1);
            let range = format!("bytes={}-{}", start, end);

            match client.get(&url_owned).header("Range", range).send().await {
                Ok(r) => {
                    let mut stream = r.bytes_stream();
                    while let Some(chunk) = stream.next().await {
                        match chunk {
                            Ok(bytes) => {
                                start += bytes.len() as u64;
                                if tx.send(bytes.to_vec()).await.is_err() {
                                    return;
                                }
                            }
                            Err(e) => {
                                eprintln!("[FETCHER] Chunk error: {e}");
                                tokio::time::sleep(Duration::from_secs(1)).await;
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[FETCHER] HTTP error: {e}");
                    tokio::time::sleep(Duration::from_secs(2)).await;
                }
            }
        }

        println!("[FETCHER] ✅ Finished downloading stream.");
    });

    tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if ffmpeg_stdin.write_all(&buf).await.is_err() {
                break;
            }
        }
        let _ = ffmpeg_stdin.shutdown().await;
        println!("[FEEDER] Input stream closed");
    });

    Ok((child, ffmpeg_stdout))
}
