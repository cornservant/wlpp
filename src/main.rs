#[macro_use]
extern crate log;

use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use smithay_client_toolkit::{
    delegate_registry,
    dispatch2::Dispatch2,
    globals::GlobalData,
    output::{OutputHandler, OutputInfo, OutputState},
    reexports::{calloop::EventLoop, calloop_wayland_source::WaylandSource},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{SeatHandler, SeatState, pointer::PointerHandler},
};
use wayland_client::{
    Connection, Dispatch, QueueHandle,
    globals::registry_queue_init,
    protocol::{wl_output::WlOutput, wl_pointer::WlPointer},
};
use wayland_protocols::ext::{
    image_capture_source::v1::client::{
        ext_image_capture_source_v1::ExtImageCaptureSourceV1,
        ext_output_image_capture_source_manager_v1::ExtOutputImageCaptureSourceManagerV1,
    },
    image_copy_capture::v1::client::{
        ext_image_copy_capture_cursor_session_v1::ExtImageCopyCaptureCursorSessionV1,
        ext_image_copy_capture_manager_v1::ExtImageCopyCaptureManagerV1,
    },
};

fn print_usage(prog: &str) {
    println!(
        r#"Usage: {prog} [OPTION]

A wayland client that prints out the pointer position as JSON.

Options:
  --watch   Continuously print the pointer position
  --help    Print help
"#
    );
}

fn main() {
    env_logger::init();

    let mut args = VecDeque::from_iter(std::env::args());
    let prog = args.pop_front().unwrap();
    let mut watch = false;
    match args.make_contiguous() {
        [flag] if flag == "--help" => {
            print_usage(&prog);
            std::process::exit(0);
        }
        [flag] if flag == "--watch" => {
            watch = true;
        }
        [] => {}
        _ => {
            print_usage(&prog);
            std::process::exit(1);
        }
    }
    warn!("{:?}", args);

    let conn = Connection::connect_to_env().unwrap();

    let (globals, event_queue) = registry_queue_init(&conn).unwrap();
    let mut event_loop: EventLoop<Client> =
        EventLoop::try_new().expect("Failed to initialize the event loop!");
    let loop_handle = event_loop.handle();
    let qh: QueueHandle<Client> = event_queue.handle();
    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle)
        .unwrap();

    let seat_state = SeatState::new(&globals, &qh);
    let image_capture_source_manager: ExtOutputImageCaptureSourceManagerV1 =
        globals.bind(&qh, 1..=1, GlobalData).unwrap();
    let copy_capture_manager: ExtImageCopyCaptureManagerV1 =
        globals.bind(&qh, 1..=1, GlobalData).unwrap();

    let mut client = Client {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        output_image_capture_source_manager: image_capture_source_manager,
        image_copy_capture_manager: copy_capture_manager,
        seat_state,
        pointer: None,
        waiting_for_pointer: VecDeque::default(),
        output_for_session: HashMap::default(),
        result: None,
        exit: false,
        watch,
    };

    loop {
        event_loop
            .dispatch(Duration::from_millis(16), &mut client)
            .unwrap();

        if let Some(PointerPositionResult { info, x, y }) = client.result.take() {
            let name = info
                .as_ref()
                .and_then(|info| info.name.clone())
                .unwrap_or("Unknown".into());
            let description = info
                .as_ref()
                .and_then(|info| info.description.clone())
                .unwrap_or("Unknown".into());
            println!(
                r#"{{"output": {{"name": {:?}, "description": {:?}}}, "x": {}, "y": {}}}"#,
                name, description, x, y
            );
        }

        if client.exit {
            break;
        }
    }
}

struct Client {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    pointer: Option<WlPointer>,
    output_image_capture_source_manager: ExtOutputImageCaptureSourceManagerV1,
    image_copy_capture_manager: ExtImageCopyCaptureManagerV1,
    waiting_for_pointer: VecDeque<WlOutput>,
    output_for_session: HashMap<ExtImageCopyCaptureCursorSessionV1, WlOutput>,
    result: Option<PointerPositionResult>,
    exit: bool,
    watch: bool,
}

struct PointerPositionResult {
    info: Option<OutputInfo>,
    x: i32,
    y: i32,
}

impl Client {
    fn listen_on(&mut self, output: WlOutput, qh: &QueueHandle<Self>) {
        if let Some(pointer) = &self.pointer {
            let capture_source = self
                .output_image_capture_source_manager
                .create_source(&output, &qh, GlobalData);
            let capture_cursor_session = self
                .image_copy_capture_manager
                .create_pointer_cursor_session(&capture_source, pointer, qh, GlobalData);
            debug!("listening to output: {:?}", output);
            self.output_for_session
                .insert(capture_cursor_session, output);
        } else {
            debug!("queued output: {:?}", output);
            self.waiting_for_pointer.push_back(output);
        }
    }
}

delegate_registry!(Client);

impl ProvidesRegistryState for Client {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![SeatState];
}

smithay_client_toolkit::delegate_dispatch2!(Client);

impl OutputHandler for Client {
    fn output_state(&mut self) -> &mut smithay_client_toolkit::output::OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        self.listen_on(output, qh);
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _output: wayland_client::protocol::wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wayland_client::protocol::wl_output::WlOutput,
    ) {
        let sessions_to_be_removed = Vec::from_iter(
            self.output_for_session
                .iter()
                .filter_map(|(session, o)| (*o == output).then_some(session.clone())),
        );
        for session in sessions_to_be_removed {
            self.output_for_session.remove(&session);
        }
        for idx in (0..self.waiting_for_pointer.len()).rev() {
            if self.waiting_for_pointer[idx] == output {
                self.waiting_for_pointer.remove(idx);
            }
        }
    }
}

impl PointerHandler for Client {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &WlPointer,
        _events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
    }
}

impl SeatHandler for Client {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }

    fn new_seat(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wayland_client::protocol::wl_seat::WlSeat,
    ) {
    }

    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wayland_client::protocol::wl_seat::WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        if capability == smithay_client_toolkit::seat::Capability::Pointer && self.pointer.is_none()
        {
            let pointer = self
                .seat_state
                .get_pointer(qh, &seat)
                .expect("Failed to create pointer");

            self.pointer = Some(pointer);
            let waiting_for_pointer =
                std::mem::replace(&mut self.waiting_for_pointer, VecDeque::new());
            for output in waiting_for_pointer {
                self.listen_on(output, qh);
            }
            assert!(self.waiting_for_pointer.is_empty());
        }
    }

    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wayland_client::protocol::wl_seat::WlSeat,
        capability: smithay_client_toolkit::seat::Capability,
    ) {
        if capability == smithay_client_toolkit::seat::Capability::Pointer && self.pointer.is_some()
        {
            let _ = self.pointer.take();
        }
    }

    fn remove_seat(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wayland_client::protocol::wl_seat::WlSeat,
    ) {
    }
}

impl Dispatch2<ExtImageCaptureSourceV1, Client> for GlobalData {
    fn event(
        &self,
        _: &mut Client,
        _: &ExtImageCaptureSourceV1,
        _: <ExtImageCaptureSourceV1 as wayland_client::Proxy>::Event,
        _: &Connection,
        _: &QueueHandle<Client>,
    ) {
    }
}

impl Dispatch<ExtImageCopyCaptureManagerV1, GlobalData> for Client {
    fn event(
        _state: &mut Self,
        _proxy: &ExtImageCopyCaptureManagerV1,
        event: <ExtImageCopyCaptureManagerV1 as wayland_client::Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        match event {
            _ => warn!(
                "[ext_image_copy_capture_manager_v1] unknown event: {:?}",
                event
            ),
        }
    }
}

impl Dispatch<ExtImageCopyCaptureCursorSessionV1, GlobalData> for Client {
    fn event(
        state: &mut Self,
        proxy: &ExtImageCopyCaptureCursorSessionV1,
        event: <ExtImageCopyCaptureCursorSessionV1 as wayland_client::Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
        let output = state.output_for_session.get(proxy).unwrap();
        let info = state.output_state.info(output);

        match event {
            wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_cursor_session_v1::Event::Enter => {},
            wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_cursor_session_v1::Event::Leave => {},
            wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_cursor_session_v1::Event::Position { x, y } => {
                state.exit = !state.watch;
                state.result = Some(PointerPositionResult { info, x, y });
            },
            wayland_protocols::ext::image_copy_capture::v1::client::ext_image_copy_capture_cursor_session_v1::Event::Hotspot { .. } => {
            },
            _ => {
                warn!("[ext_image_copy_capture_cursor_session_v1] unknown event {:?}", event);
            },
        }
    }
}

impl Dispatch<ExtOutputImageCaptureSourceManagerV1, GlobalData> for Client {
    fn event(
        _state: &mut Self,
        _proxy: &ExtOutputImageCaptureSourceManagerV1,
        _event: <ExtOutputImageCaptureSourceManagerV1 as wayland_client::Proxy>::Event,
        _data: &GlobalData,
        _conn: &Connection,
        _qhandle: &QueueHandle<Self>,
    ) {
    }
}
