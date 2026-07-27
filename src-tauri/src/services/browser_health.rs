use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use tracing::{debug, info, warn};

use crate::browser::BrowserManager;
use crate::runtime::FacebookRuntime;

const HEALTH_POLL_INTERVAL: Duration = Duration::from_secs(30);
const INITIAL_HEALTH_BACKOFF: Duration = Duration::from_secs(5);

pub struct BrowserHealthService {
    shutdown: Arc<AtomicBool>,
}

impl BrowserHealthService {
    pub fn spawn(
        browser_manager: Arc<BrowserManager>,
        facebook_runtime: Arc<FacebookRuntime>,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            shutdown: Arc::new(AtomicBool::new(false)),
        });

        let shutdown_flag = service.shutdown.clone();
        let manager = browser_manager.clone();
        let runtime = facebook_runtime.clone();

        thread::spawn(move || {
            let mut backoff = INITIAL_HEALTH_BACKOFF;
            loop {
                if shutdown_flag.load(Ordering::SeqCst) {
                    info!("browser health monitor stopped");
                    break;
                }

                if !manager.snapshot().enabled {
                    thread::sleep(HEALTH_POLL_INTERVAL);
                    continue;
                }

                if !manager.snapshot().sidecar_running {
                    thread::sleep(HEALTH_POLL_INTERVAL);
                    continue;
                }

                let snapshot = manager.snapshot();
                if snapshot.status.is_operational() || snapshot.status.is_terminal_error() {
                    match manager.health_check() {
                        Ok(updated) => {
                            if updated.status.is_operational() {
                                backoff = INITIAL_HEALTH_BACKOFF;
                            }
                            if updated.status == crate::browser::BrowserRuntimeStatus::BrowserReady
                            {
                                let _ = runtime.session.check_session();
                            }
                        }
                        Err(err) => {
                            warn!(error = %err, "periodic browser health check failed");
                            thread::sleep(backoff);
                            backoff = (backoff * 2).min(Duration::from_secs(120));
                            continue;
                        }
                    }
                } else if snapshot.status == crate::browser::BrowserRuntimeStatus::BrowserCrashed
                {
                    debug!("browser crashed — manager auto-restart handles recovery");
                }

                thread::sleep(HEALTH_POLL_INTERVAL);
            }
        });

        service
    }

    pub fn stop(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_poll_interval_is_reasonable() {
        assert!(HEALTH_POLL_INTERVAL.as_secs() >= 10);
    }
}
