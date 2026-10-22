//! Built-in CI: pipeline config, the job queue, and the runner.

pub mod cache;
pub mod config;
pub mod http;
pub mod jobs;
pub mod queue;
pub mod reports;
pub mod runner;
pub mod trigger;

use std::time::Duration;

use crate::AppState;

/// Background upkeep: put jobs whose runner went quiet back in the queue, and
/// start scheduled runs once a minute.
pub async fn upkeep(st: AppState) {
    let (mut minute, mut hour) = (0, 0);
    loop {
        match st.db.call(|c| Ok(queue::requeue_expired(c)?)).await {
            Ok(n) if n > 0 => st.ci_wake.notify_waiters(),
            Ok(_) => {}
            Err(e) => eprintln!("ci requeue: {}", e.msg),
        }
        let now = crate::db::now();
        if now / 3600 != hour {
            hour = now / 3600;
            match jobs::sweep(&st).await {
                Ok(n) if n > 0 => println!("ci: removed {n} old log or artifact file(s)"),
                Ok(_) => {}
                Err(e) => eprintln!("ci sweep: {}", e.msg),
            }
            if let Err(e) = cache::sweep(&st).await {
                eprintln!("ci cache sweep: {}", e.msg);
            }
        }
        if now / 60 != minute {
            minute = now / 60;
            if let Err(e) = trigger::tick(&st, now).await {
                eprintln!("ci schedule: {}", e.msg);
            }
            if let Err(e) = crate::mergequeue::tick(&st).await {
                eprintln!("merge queue: {}", e.msg);
            }
        }
        tokio::time::sleep(Duration::from_secs(15)).await;
    }
}
