use std::env;
use json::{JsonValue, object};
use lambda_extension::{service_fn, Error, Extension, LambdaLog, LambdaLogRecord, SharedService, LogBuffering};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tokio::sync::mpsc::{Sender, channel};
use tracing::error;

const ADDRESS_ENV_NAME: &str = "LOG_STORE_ADDRESS";

async fn handler(logs: Vec<LambdaLog>, sender: Sender<JsonValue>) -> Result<(), Error> {
    for log in logs {
        let mut json = object! {
            "t": log.time.timestamp_millis()
        };

        match log.record {
            LambdaLogRecord::Function(record) => {
                json.insert("type", "function")?;

                // attempt to parse the record as JSON
                if let Ok(json_value) = json::parse(record.as_str()) {
                    match json_value {
                        JsonValue::Object(obj) => {
                            for (k,v) in obj.iter() {
                                json.insert(k, v.to_owned())?;
                            }
                        }
                        JsonValue::Null => {
                            // skip entirely
                        }
                         _ => {
                             json.insert("record", json_value)?;
                         }
                    }
                } else {
                    json.insert("record", record)?;
                }
            },
            // LambdaLogRecord::Extension(record) => {
            //     json.insert("type", "extension")?;
            //     json.insert("record", record)?;
            // },
            LambdaLogRecord::PlatformStart {request_id} => {
                json.insert("type", "platform_start")?;
                json.insert("request_id", request_id)?;
            }
            LambdaLogRecord::PlatformEnd {request_id} => {
                json.insert("type", "platform_end")?;
                json.insert("request_id", request_id)?;
            }
            LambdaLogRecord::PlatformFault(record) => {
                json.insert("type", "platform_fault")?;
                json.insert("record", record)?;
            }
            LambdaLogRecord::PlatformReport {request_id, metrics} => {
                json.insert("type", "platform_report")?;
                json.insert("request_id", request_id)?;
                json.insert("duration_ms", metrics.duration_ms)?;
                json.insert("billed_duration_ms", metrics.billed_duration_ms)?;
                json.insert("memory_size_mb", metrics.memory_size_mb)?;
                json.insert("max_memory_used_mb", metrics.max_memory_used_mb)?;
                json.insert("init_duration_ms", metrics.init_duration_ms)?;
            }
            _ => (),
        }

        sender.send(json).await?;
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        // disable printing the name of the module in every log line.
        .with_target(false)
        // disabling time is handy because CloudWatch will add the ingestion time.
        .without_time()
        .init();

    let (_, log_store_address) = env::vars().find(|(k, _)| k == ADDRESS_ENV_NAME)
        .ok_or_else(|| format!("Unable to find environment variable: {}", ADDRESS_ENV_NAME))?;

    let (sender, mut recver) = channel(1024);

    let logs_processor = SharedService::new(service_fn(move |logs| {
        let sender_clone = sender.clone();

        async move {
            handler(logs, sender_clone).await
        }
    }));

    tokio::spawn(async move {
        let stream = match TcpStream::connect(log_store_address.as_str()).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Error connecting to log-store instance at {}: {}", log_store_address, e);
                eprintln!("Logs will be written to STDOUT instead");
                while let Some(json) = recver.recv().await {
                    println!("{}", json);
                }
                return;
            }
        };

        let mut stream = BufWriter::new(stream);

        while let Some(json) = recver.recv().await {
            // convert to a string
            let json_str = format!("{}\n", json);

            if let Err(e) = stream.write_all(json_str.as_bytes()).await {
                eprintln!("Error writing to log-store: {}", e);
                continue
            }

            if let Err(e) = stream.flush().await {
                eprintln!("Error flushing stream: {}", e);
                continue
            }
        }

        if let Err(e) = stream.shutdown().await {
            error!("Error shutting down stream: {}", e);
        }
    });

    // set to the min, to try and speed up logging
    let buffering = LogBuffering {
        timeout_ms: 25,
        max_bytes: 262_144,
        max_items: 1_000,
    };

    Extension::new()
        .with_log_buffering(buffering)
        .with_logs_processor(logs_processor)
        .run().await?;

    Ok(())
}
