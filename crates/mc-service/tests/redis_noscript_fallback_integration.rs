//! `FencedRedisClient::store_meeting_state` EVALSHA -> EVAL fallback on a
//! `NOSCRIPT` reply.
//!
//! The fallback hinges on classifying the server's `-NOSCRIPT` error reply via
//! `redis::ErrorKind`, whose shape changed across redis-rs majors (0.x
//! `ErrorKind::NoScriptError`, 1.x `ErrorKind::Server(ServerErrorKind::NoScript)`).
//! A compile fix alone can't show the classification still matches, so these
//! tests drive the real client against a minimal in-process RESP server that
//! answers `EVALSHA` with whatever reply the test chooses and records every
//! command it receives.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::{Arc, Mutex};

use mc_service::redis::client::FencedRedisClient;
use mc_service::redis::lua_scripts::FENCED_HSET;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

type Commands = Arc<Mutex<Vec<Vec<String>>>>;

/// Reads one RESP array-of-bulk-strings command; `None` on EOF.
async fn read_command<R: AsyncBufReadExt + Unpin>(reader: &mut R) -> Option<Vec<String>> {
    let mut line = String::new();
    if reader.read_line(&mut line).await.ok()? == 0 {
        return None;
    }
    let count: usize = line.trim_end().strip_prefix('*')?.parse().ok()?;
    let mut args = Vec::with_capacity(count);
    for _ in 0..count {
        line.clear();
        reader.read_line(&mut line).await.ok()?;
        let len: usize = line.trim_end().strip_prefix('$')?.parse().ok()?;
        let mut buf = vec![0u8; len + 2]; // payload + CRLF
        reader.read_exact(&mut buf).await.ok()?;
        buf.truncate(len);
        args.push(String::from_utf8(buf).ok()?);
    }
    Some(args)
}

/// Upper-cased command name (first RESP argument).
fn command_name(cmd: &[String]) -> String {
    cmd.first()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or_default()
}

/// Starts a fake Redis that replies `evalsha_reply` to `EVALSHA`, `:1` (the
/// fenced-HSET success code) to `EVAL`, and `+OK` to anything else (connection
/// setup such as `CLIENT SETINFO`). Returns its URL and the command log.
async fn fake_redis(evalsha_reply: &'static str) -> (String, Commands) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("redis://{}", listener.local_addr().unwrap());
    let commands: Commands = Arc::default();
    let log = Arc::clone(&commands);
    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            let log = Arc::clone(&log);
            tokio::spawn(async move {
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read);
                while let Some(cmd) = read_command(&mut reader).await {
                    let reply = match command_name(&cmd).as_str() {
                        "EVALSHA" => evalsha_reply,
                        "EVAL" => ":1\r\n",
                        _ => "+OK\r\n",
                    };
                    log.lock().unwrap().push(cmd);
                    if write.write_all(reply.as_bytes()).await.is_err() {
                        return;
                    }
                }
            });
        }
    });
    (url, commands)
}

/// The script-invocation commands the server saw, in order.
fn script_commands(commands: &Commands) -> Vec<Vec<String>> {
    commands
        .lock()
        .unwrap()
        .iter()
        .filter(|c| matches!(command_name(c).as_str(), "EVALSHA" | "EVAL"))
        .cloned()
        .collect()
}

const FIELDS: &[(&str, &str)] = &[("status", "active"), ("host", "mc-1")];

/// Args after the script/sha: numkeys, both keys, generation, field/value pairs.
fn expected_tail() -> Vec<String> {
    [
        "2",
        "meeting:m-1:generation",
        "meeting:m-1:state",
        "7",
        "status",
        "active",
        "host",
        "mc-1",
    ]
    .map(String::from)
    .to_vec()
}

#[tokio::test]
async fn store_meeting_state_falls_back_to_eval_on_noscript() {
    let (url, commands) = fake_redis("-NOSCRIPT No matching script. Please use EVAL.\r\n").await;
    let client = FencedRedisClient::new(&url).await.unwrap();

    client
        .store_meeting_state("m-1", 7, FIELDS)
        .await
        .expect("NOSCRIPT must fall back to EVAL, not surface as an error");

    let sent = script_commands(&commands);
    assert_eq!(sent.len(), 2, "expected EVALSHA then EVAL, got {sent:?}");

    let sha = redis::Script::new(FENCED_HSET).get_hash().to_string();
    let mut evalsha = vec!["EVALSHA".to_string(), sha];
    evalsha.extend(expected_tail());
    assert_eq!(sent[0], evalsha);

    let mut eval = vec!["EVAL".to_string(), FENCED_HSET.to_string()];
    eval.extend(expected_tail());
    assert_eq!(sent[1], eval);
}

#[tokio::test]
async fn store_meeting_state_uses_cached_script_without_eval() {
    let (url, commands) = fake_redis(":1\r\n").await;
    let client = FencedRedisClient::new(&url).await.unwrap();

    client.store_meeting_state("m-1", 7, FIELDS).await.unwrap();

    let sent = script_commands(&commands);
    assert_eq!(
        sent.len(),
        1,
        "cached script must not re-send EVAL: {sent:?}"
    );
    assert_eq!(sent[0][0], "EVALSHA");
}

#[tokio::test]
async fn store_meeting_state_other_error_does_not_fall_back() {
    let (url, commands) = fake_redis("-ERR something else\r\n").await;
    let client = FencedRedisClient::new(&url).await.unwrap();

    let result = client.store_meeting_state("m-1", 7, FIELDS).await;

    assert!(result.is_err(), "a non-NOSCRIPT error must surface");
    let sent = script_commands(&commands);
    assert_eq!(sent.len(), 1, "only NOSCRIPT may trigger EVAL: {sent:?}");
    assert_eq!(sent[0][0], "EVALSHA");
}
