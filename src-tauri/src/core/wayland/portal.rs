use super::{compose, decode_frame, pointer_target, Frame};
use ashpd::desktop::{
    remote_desktop::{Axis, DeviceType, RemoteDesktop, SelectDevicesOptions},
    screencast::{CursorMode, Screencast, SelectSourcesOptions, SourceType},
    Session,
};
use futures_util::{FutureExt, StreamExt};
use image::RgbaImage;
use pipewire::{self as pw, properties::properties, spa};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{mpsc, OnceLock},
    time::{Duration, Instant},
};

enum Request {
    Capture(mpsc::SyncSender<Result<RgbaImage, String>>),
    Scroll(i32, i32, i32, mpsc::SyncSender<Result<(), String>>),
    Release,
}

enum Sharing {
    Remote(RemoteDesktop, Session<RemoteDesktop>, bool),
    Screen(Session<Screencast>),
}

impl Sharing {
    fn close(&self) {
        let _ = tauri::async_runtime::block_on(async {
            match self {
                Self::Remote(_, session, _) => session.close().await,
                Self::Screen(session) => session.close().await,
            }
        });
    }
}

impl Drop for Sharing {
    fn drop(&mut self) {
        self.close();
    }
}

impl Request {
    fn fail(self, error: &str) {
        match self {
            Self::Capture(reply) => {
                let _ = reply.send(Err(error.into()));
            }
            Self::Scroll(_, _, _, reply) => {
                let _ = reply.send(Err(error.into()));
            }
            Self::Release => {}
        }
    }
}

fn sender() -> &'static mpsc::Sender<Request> {
    static SENDER: OnceLock<mpsc::Sender<Request>> = OnceLock::new();
    SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            while let Ok(request) = receiver.recv() {
                if matches!(request, Request::Release) {
                    continue;
                }
                run_session(request, &receiver);
            }
        });
        sender
    })
}

pub fn capture() -> Result<RgbaImage, String> {
    let (reply, result) = mpsc::sync_channel(1);
    sender()
        .send(Request::Capture(reply))
        .map_err(|error| error.to_string())?;
    result.recv().map_err(|error| error.to_string())?
}

pub fn scroll(x: i32, y: i32, delta: i32) -> Result<(), String> {
    let (reply, result) = mpsc::sync_channel(1);
    sender()
        .send(Request::Scroll(x, y, delta, reply))
        .map_err(|error| error.to_string())?;
    result.recv().map_err(|error| error.to_string())?
}

pub fn release() {
    let _ = sender().send(Request::Release);
}

fn run_session(first: Request, receiver: &mpsc::Receiver<Request>) {
    // PipeWire 的对象和回调固定在这个线程，窗口线程不持有原生指针。
    let mut first = Some(first);
    let result = (|| -> Result<(), String> {
        let (sharing, streams, fd) = tauri::async_runtime::block_on(async {
            let portal = RemoteDesktop::new().await.ok();
            let screencast = Screencast::new().await?;
            let sources = || {
                SelectSourcesOptions::default()
                    .set_sources(Some(SourceType::Monitor.into()))
                    .set_multiple(true)
                    .set_cursor_mode(CursorMode::Hidden)
            };
            // 部分 wlroots 门户只有屏幕共享，仍可截图和手动长截图。
            let remote_supported = if let Some(portal) = &portal {
                portal
                    .available_device_types()
                    .await
                    .is_ok_and(|devices| devices.contains(DeviceType::Pointer))
            } else {
                false
            };
            if !remote_supported {
                let session = screencast.create_session(Default::default()).await?;
                let result = async {
                    screencast
                        .select_sources(&session, sources())
                        .await?
                        .response()?;
                    let response = screencast
                        .start(&session, None, Default::default())
                        .await?
                        .response()?;
                    let fd = screencast
                        .open_pipe_wire_remote(&session, Default::default())
                        .await?;
                    Ok::<_, ashpd::Error>((response.streams().to_vec(), fd))
                }
                .await;
                return match result {
                    Ok((streams, fd)) => Ok((Sharing::Screen(session), streams, fd)),
                    Err(error) => {
                        let _ = session.close().await;
                        Err(error)
                    }
                };
            }
            let portal = portal.ok_or(ashpd::Error::NoResponse)?;
            let session = portal.create_session(Default::default()).await?;
            let result = async {
                portal
                    .select_devices(
                        &session,
                        SelectDevicesOptions::default()
                            .set_devices(Some(DeviceType::Pointer.into())),
                    )
                    .await?
                    .response()?;
                screencast
                    .select_sources(&session, sources())
                    .await?
                    .response()?;
                let response = portal
                    .start(&session, None, Default::default())
                    .await?
                    .response()?;
                let fd = screencast
                    .open_pipe_wire_remote(&session, Default::default())
                    .await?;
                Ok::<_, ashpd::Error>((
                    response.streams().to_vec(),
                    fd,
                    response.devices().contains(DeviceType::Pointer),
                ))
            }
            .await;
            match result {
                Ok((streams, fd, pointer_allowed)) => Ok((
                    Sharing::Remote(portal, session, pointer_allowed),
                    streams,
                    fd,
                )),
                Err(error) => {
                    let _ = session.close().await;
                    Err(error)
                }
            }
        })
        .map_err(|error| format!("桌面共享授权失败：{error}"))?;
        let mut closed = false;
        let mut closed_events = tauri::async_runtime::block_on(async {
            match &sharing {
                Sharing::Remote(_, session, _) => session
                    .receive_closed()
                    .await
                    .map(|events| events.map(|_| ()).boxed()),
                Sharing::Screen(session) => session
                    .receive_closed()
                    .await
                    .map(|events| events.map(|_| ()).boxed()),
            }
        })
        .map_err(|error| error.to_string())?;
        (|| -> Result<(), String> {
            if streams.is_empty() {
                return Err("没有授权共享的屏幕".into());
            }
            pw::init();
            let mainloop =
                pw::main_loop::MainLoopRc::new(None).map_err(|error| error.to_string())?;
            let context =
                pw::context::ContextRc::new(&mainloop, None).map_err(|error| error.to_string())?;
            let core = context
                .connect_fd_rc(fd, None)
                .map_err(|error| error.to_string())?;
            let frames = Rc::new(RefCell::new(Vec::<Option<Frame>>::new()));
            let stream_error = Rc::new(RefCell::new(None::<String>));
            let _core_listener = core
                .add_listener_local()
                .error({
                    let failure = stream_error.clone();
                    move |_, _, _, message| {
                        *failure.borrow_mut() = Some(format!("屏幕采集连接已断开：{message}"));
                    }
                })
                .register();
            let mut video_streams = Vec::new();
            let mut listeners = Vec::new();
            let mut original_layout = None;
            for source in &streams {
                let index = frames.borrow().len();
                frames.borrow_mut().push(None);
                let frame_store = frames.clone();
                let failure = stream_error.clone();
                let node = source.pipe_wire_node_id();
                let logical_size = source.size();
                let stream = pw::stream::StreamRc::new(
                    core.clone(),
                    "Monkey Thief desktop",
                    properties! {
                        *pw::keys::MEDIA_TYPE => "Video",
                        *pw::keys::MEDIA_CATEGORY => "Capture",
                        *pw::keys::MEDIA_ROLE => "Screen",
                    },
                )
                .map_err(|error| error.to_string())?;
                let listener = stream
                    .add_local_listener_with_user_data(spa::param::video::VideoInfoRaw::default())
                    .state_changed({
                        let failure = failure.clone();
                        move |_, _, _, state| match state {
                            pw::stream::StreamState::Error(error) => {
                                *failure.borrow_mut() = Some(format!("屏幕视频流已中断：{error}"));
                            }
                            pw::stream::StreamState::Unconnected => {
                                *failure.borrow_mut() = Some("屏幕视频流已断开".into());
                            }
                            _ => {}
                        }
                    })
                    .param_changed(move |_, format, id, param| {
                        if id == spa::param::ParamType::Format.as_raw() {
                            if let Some(param) = param {
                                if let Err(error) = format.parse(param) {
                                    *failure.borrow_mut() =
                                        Some(format!("屏幕视频格式无效：{error}"));
                                }
                            }
                        }
                    })
                    .process({
                        let failure = stream_error.clone();
                        move |stream, format| {
                            let Some(mut buffer) = stream.dequeue_buffer() else {
                                return;
                            };
                            let Some(data) = buffer.datas_mut().first_mut() else {
                                return;
                            };
                            let offset = data.chunk().offset() as usize;
                            let size = data.chunk().size() as usize;
                            let stride = data.chunk().stride();
                            if size == 0 {
                                return;
                            }
                            let Some(bytes) = data.data() else {
                                return;
                            };
                            use spa::param::video::VideoFormat;
                            let bgr =
                                matches!(format.format(), VideoFormat::BGRx | VideoFormat::BGRA);
                            let alpha =
                                matches!(format.format(), VideoFormat::RGBA | VideoFormat::BGRA);
                            match decode_frame(
                                bytes,
                                format.size().width,
                                format.size().height,
                                offset,
                                size,
                                stride,
                                bgr,
                                alpha,
                            ) {
                                Ok(image) => {
                                    frame_store.borrow_mut()[index] = Some(Frame {
                                        node,
                                        logical_size,
                                        image,
                                    })
                                }
                                Err(error) => *failure.borrow_mut() = Some(error),
                            }
                        }
                    })
                    .register()
                    .map_err(|error| error.to_string())?;
                let format = spa::pod::object!(
                    spa::utils::SpaTypes::ObjectParamFormat,
                    spa::param::ParamType::EnumFormat,
                    spa::pod::property!(
                        spa::param::format::FormatProperties::MediaType,
                        Id,
                        spa::param::format::MediaType::Video
                    ),
                    spa::pod::property!(
                        spa::param::format::FormatProperties::MediaSubtype,
                        Id,
                        spa::param::format::MediaSubtype::Raw
                    ),
                    spa::pod::property!(
                        spa::param::format::FormatProperties::VideoFormat,
                        Choice,
                        Enum,
                        Id,
                        spa::param::video::VideoFormat::BGRx,
                        spa::param::video::VideoFormat::BGRx,
                        spa::param::video::VideoFormat::BGRA,
                        spa::param::video::VideoFormat::RGBx,
                        spa::param::video::VideoFormat::RGBA
                    )
                );
                let bytes = spa::pod::serialize::PodSerializer::serialize(
                    std::io::Cursor::new(Vec::new()),
                    &spa::pod::Value::Object(format),
                )
                .map_err(|error| error.to_string())?
                .0
                .into_inner();
                let mut parameters =
                    [spa::pod::Pod::from_bytes(&bytes).ok_or("无法构造屏幕视频格式")?];
                stream
                    .connect(
                        spa::utils::Direction::Input,
                        Some(node),
                        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
                        &mut parameters,
                    )
                    .map_err(|error| error.to_string())?;
                listeners.push(listener);
                video_streams.push(stream);
            }
            loop {
                mainloop.loop_().iterate(Duration::from_millis(10).into());
                // 事件流借用会话，在采集线程轮询，避免把借用移交到独立任务。
                closed = closed || closed_events.next().now_or_never().is_some();
                let request = first.take().or_else(|| receiver.try_recv().ok());
                let Some(request) = request else {
                    continue;
                };
                if matches!(request, Request::Release) {
                    return Ok(());
                }
                if closed {
                    request.fail("屏幕共享已被系统停止，请重新开始截图");
                    // 保持关闭状态直到调用方释放，避免长截图循环重新弹授权窗口。
                    continue;
                }
                match request {
                    Request::Capture(reply) => {
                        let started = Instant::now();
                        while started.elapsed() < Duration::from_secs(3) {
                            mainloop.loop_().iterate(Duration::from_millis(10).into());
                            closed = closed || closed_events.next().now_or_never().is_some();
                            if closed
                                || stream_error.borrow().is_some()
                                || (started.elapsed() >= Duration::from_millis(80)
                                    && frames.borrow().iter().all(Option::is_some))
                            {
                                break;
                            }
                        }
                        let failure = stream_error.borrow().clone();
                        let result = if closed {
                            Err("屏幕共享已被系统停止，请重新开始截图".into())
                        } else if let Some(error) = failure {
                            Err(error)
                        } else {
                            let frames = frames.borrow();
                            let result = frames
                                .iter()
                                .map(|frame| {
                                    frame.as_ref().ok_or_else(|| {
                                        "未收到全部屏幕的视频帧，请检查 PipeWire 与桌面门户"
                                            .to_string()
                                    })
                                })
                                .collect::<Result<Vec<_>, _>>();
                            result.and_then(|frames| {
                                let layout = frames
                                    .iter()
                                    .map(|frame| (frame.node, frame.image.dimensions()))
                                    .collect::<Vec<_>>();
                                if original_layout
                                    .as_ref()
                                    .is_some_and(|original| original != &layout)
                                {
                                    let error = "共享屏幕尺寸已变化，请重新开始截图".to_string();
                                    *stream_error.borrow_mut() = Some(error.clone());
                                    return Err(error);
                                }
                                original_layout = Some(layout);
                                compose(&frames)
                            })
                        };
                        let _ = reply.send(result);
                    }
                    Request::Scroll(x, y, delta, reply) => {
                        let result = (|| {
                            if let Some(error) = stream_error.borrow().as_ref() {
                                return Err(error.clone());
                            }
                            let Sharing::Remote(portal, session, true) = &sharing else {
                                return Err("当前门户未授权鼠标输入，请手动滚动页面".into());
                            };
                            let frames = frames.borrow();
                            let frames = frames.iter().flatten().collect::<Vec<_>>();
                            let (node, x, y) = pointer_target(&frames, x, y)?;
                            tauri::async_runtime::block_on(async {
                                portal
                                    .notify_pointer_motion_absolute(
                                        session,
                                        node,
                                        x,
                                        y,
                                        Default::default(),
                                    )
                                    .await?;
                                portal
                                    .notify_pointer_axis_discrete(
                                        session,
                                        Axis::Vertical,
                                        -delta.signum() * 3,
                                        Default::default(),
                                    )
                                    .await
                            })
                            .map_err(|error| error.to_string())
                        })();
                        let _ = reply.send(result);
                    }
                    Request::Release => return Ok(()),
                }
            }
        })()
    })();
    if let Err(error) = result {
        if let Some(request) = first.take() {
            request.fail(&error);
        }
    }
}
