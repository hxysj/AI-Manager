use super::*;
use futures_util::stream::{self, Stream};
use futures_util::FutureExt;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::Instant;

const SLICE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Settings {
    pub(super) send_mib_per_second: u64,
    pub(super) max_concurrent_files: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            send_mib_per_second: 2,
            max_concurrent_files: 1,
        }
    }
}

struct Budget {
    settings: Settings,
    tokens: f64,
    updated: Instant,
    active_files: usize,
    active_uploads: usize,
}

pub(super) struct Control {
    budget: StdMutex<Budget>,
    changed: Notify,
}

impl Default for Control {
    fn default() -> Self {
        Self {
            budget: StdMutex::new(Budget {
                settings: Settings::default(),
                tokens: 0.0,
                updated: Instant::now(),
                active_files: 0,
                active_uploads: 0,
            }),
            changed: Notify::new(),
        }
    }
}

impl Control {
    pub(super) fn configure(&self, settings: Settings) {
        let mut budget = self.budget.lock().unwrap();
        budget.settings = settings;
        budget.tokens = 0.0;
        budget.updated = Instant::now();
        drop(budget);
        self.changed.notify_waiters();
    }

    pub(super) async fn acquire_file(self: &Arc<Self>) -> FilePermit {
        self.acquire_slot(false).await
    }

    pub(super) async fn acquire_upload(self: &Arc<Self>) -> FilePermit {
        self.acquire_slot(true).await
    }

    async fn acquire_slot(self: &Arc<Self>, receiving: bool) -> FilePermit {
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let mut budget = self.budget.lock().unwrap();
                let maximum = budget.settings.max_concurrent_files;
                let active = if receiving { &mut budget.active_uploads } else { &mut budget.active_files };
                if *active < maximum {
                    *active += 1;
                    return FilePermit { control: self.clone(), receiving };
                }
            }
            notified.await;
        }
    }

    async fn acquire_bytes(&self, size: usize) {
        debug_assert!(size <= SLICE_BYTES);
        loop {
            let notified = self.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let delay = {
                let mut budget = self.budget.lock().unwrap();
                let now = Instant::now();
                let rate = (budget.settings.send_mib_per_second * 1024 * 1024) as f64;
                // 全部文件共用预算，空闲期间最多积攒一个小片，避免整块突发。
                budget.tokens = (budget.tokens
                    + now.duration_since(budget.updated).as_secs_f64() * rate)
                    .min(SLICE_BYTES as f64);
                budget.updated = now;
                if budget.tokens >= size as f64 {
                    budget.tokens -= size as f64;
                    return;
                }
                Duration::from_secs_f64((size as f64 - budget.tokens) / rate)
            };
            futures_util::select! {
                _ = tokio::time::sleep(delay).fuse() => {},
                _ = notified.fuse() => {},
            }
        }
    }

    pub(super) async fn receive_bytes(&self, mut size: usize) {
        // 接收帧可能大于发送片，分片计入同一预算，避免接收绕过总限速。
        while size > 0 {
            let slice = size.min(SLICE_BYTES);
            self.acquire_bytes(slice).await;
            size -= slice;
        }
    }

    pub(super) fn bytes_stream(
        self: &Arc<Self>,
        bytes: Bytes,
    ) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static {
        stream::unfold((bytes, self.clone()), |(bytes, control)| async move {
            if bytes.is_empty() {
                return None;
            }
            let size = bytes.len().min(SLICE_BYTES);
            control.acquire_bytes(size).await;
            Some((Ok(bytes.slice(..size)), (bytes.slice(size..), control)))
        })
    }

    pub(super) async fn file_stream(
        self: &Arc<Self>,
        file: File,
    ) -> impl Stream<Item = Result<Bytes, std::io::Error>> + Send + Sync + 'static {
        let permit = self.acquire_file().await;
        let control = self.clone();
        ReaderStream::with_capacity(file, SLICE_BYTES)
            .map(move |chunk| {
                // 许可跟随响应流，完成、断开或取消时自动释放。
                let _ = &permit;
                chunk
            })
            .then(move |chunk| {
                let control = control.clone();
                async move {
                    if let Ok(bytes) = &chunk {
                        control.acquire_bytes(bytes.len()).await;
                    }
                    chunk
                }
            })
    }
}

pub(super) struct FilePermit {
    control: Arc<Control>,
    receiving: bool,
}

impl Drop for FilePermit {
    fn drop(&mut self) {
        let mut budget = self.control.budget.lock().unwrap();
        if self.receiving {
            budget.active_uploads -= 1;
        } else {
            budget.active_files -= 1;
        }
        drop(budget);
        self.control.changed.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incoming_and_outgoing_files_share_the_total_rate() {
        tauri::async_runtime::block_on(async {
            let control = Arc::new(Control::default());
            control.configure(Settings { send_mib_per_second: 1, max_concurrent_files: 1 });
            let started = Instant::now();
            let send = async {
                let stream = control.bytes_stream(Bytes::from(vec![3; 128 * 1024]));
                tokio::pin!(stream);
                let mut sent = 0;
                while let Some(bytes) = stream.next().await {
                    sent += bytes.unwrap().len();
                }
                sent
            };
            let (sent, ()) = futures_util::join!(send, control.receive_bytes(128 * 1024));
            assert_eq!(sent, 128 * 1024);
            assert!(started.elapsed() >= Duration::from_millis(240));
        });
    }

    #[test]
    fn bidirectional_transfers_do_not_wait_on_each_others_file_slots() {
        tauri::async_runtime::block_on(async {
            let control = Arc::new(Control::default());
            let outgoing = control.acquire_file().await;
            let incoming = control.acquire_upload().now_or_never().unwrap();
            let mut next_incoming = Box::pin(control.acquire_upload());
            assert!(next_incoming.as_mut().now_or_never().is_none());
            drop(incoming);
            let incoming = next_incoming.await;
            drop(outgoing);
            let _outgoing = control.acquire_file().now_or_never().unwrap();
            drop(incoming);

            let task_control = control.clone();
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let task = tauri::async_runtime::spawn(async move {
                let _permit = task_control.acquire_upload().await;
                ready_tx.send(()).unwrap();
                std::future::pending::<()>().await;
            });
            ready_rx.await.unwrap();
            task.abort();
            let _ = task.await;
            let _incoming = control.acquire_upload().now_or_never().unwrap();
        });
    }

    #[test]
    fn all_streams_share_one_bandwidth_budget_and_small_slices() {
        tauri::async_runtime::block_on(async {
            let control = Arc::new(Control::default());
            control.configure(Settings {
                send_mib_per_second: 1,
                max_concurrent_files: 2,
            });
            let started = Instant::now();
            let read = |control: Arc<Control>| async move {
                let stream = control.bytes_stream(Bytes::from(vec![7; 128 * 1024]));
                tokio::pin!(stream);
                let mut size = 0;
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.unwrap();
                    assert!(chunk.len() <= SLICE_BYTES);
                    assert!(chunk.iter().all(|byte| *byte == 7));
                    size += chunk.len();
                }
                size
            };
            let (first, second) = futures_util::join!(read(control.clone()), read(control));
            assert_eq!(first + second, 256 * 1024);
            assert!(started.elapsed() >= Duration::from_millis(240));
        });
    }

    #[test]
    fn permits_obey_live_limits_and_release_after_cancellation() {
        tauri::async_runtime::block_on(async {
            let control = Arc::new(Control::default());
            let first = control.acquire_file().await;
            let mut waiting = Box::pin(control.acquire_file());
            assert!(waiting.as_mut().now_or_never().is_none());
            control.configure(Settings {
                send_mib_per_second: 10,
                max_concurrent_files: 2,
            });
            let second = waiting.as_mut().now_or_never().unwrap();
            control.configure(Settings::default());
            let mut third = Box::pin(control.acquire_file());
            assert!(third.as_mut().now_or_never().is_none());
            drop(first);
            assert!(third.as_mut().now_or_never().is_none());
            drop(second);
            let third = third.await;
            drop(third);
            let task_control = control.clone();
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let task = tauri::async_runtime::spawn(async move {
                let _permit = task_control.acquire_file().await;
                ready_tx.send(()).unwrap();
                std::future::pending::<()>().await;
            });
            ready_rx.await.unwrap();
            task.abort();
            let _ = task.await;
            let _permit = tokio::time::timeout(Duration::from_secs(1), control.acquire_file())
                .await
                .unwrap();
        });
    }

    #[test]
    fn browser_stream_holds_shared_file_slot_until_response_is_dropped() {
        tauri::async_runtime::block_on(async {
            let path = std::env::temp_dir().join(create_id());
            tokio::fs::write(&path, vec![9; SLICE_BYTES * 2])
                .await
                .unwrap();
            let control = Arc::new(Control::default());
            let native_permit = control.acquire_file().await;
            let file = File::open(&path).await.unwrap();
            let mut browser_wait = Box::pin(control.file_stream(file));
            assert!(browser_wait.as_mut().now_or_never().is_none());
            drop(native_permit);
            let mut browser = Box::pin(browser_wait.await);
            let mut next_native = Box::pin(control.acquire_file());
            assert!(next_native.as_mut().now_or_never().is_none());
            assert_eq!(browser.next().await.unwrap().unwrap().len(), SLICE_BYTES);
            drop(browser);
            let next_native = tokio::time::timeout(Duration::from_secs(1), next_native)
                .await
                .unwrap();
            drop(next_native);
            tokio::fs::remove_file(path).await.unwrap();
        });
    }
}
