//! The window: a tao window with a wry web view serving the embedded page through the `tempester` protocol.
//! Page -> Rust: the IPC handler gets one line per command (`command::parse`). Rust -> page: `evaluate_script` with
//! the JSON of an `Event`. The window refuses to close while an OS update is being sent, unless the user confirms
//! quitting a send in one piece (which has no cancel) on the page.

mod assets;
mod script;
mod window_icon;

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, PoisonError};

use tao::dpi::LogicalSize;
use tao::event::{Event as WindowEvent, WindowEvent as Inner};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use wry::http::{header::CONTENT_TYPE, Request, Response, StatusCode};
use wry::{WebContext, WebViewBuilder};

use crate::command::{self, Command};
use crate::error::AppError;
use crate::event::{Emitter, Event};
use crate::machine::{self, connector::Connector, Worker};
use crate::storage::Storage;
use script::script_for;

const SCHEME: &str = "tempester";
/// The web view's own data (WebView2's profile: cache, local storage), inside the data folder.
const WEBVIEW_DATA_DIR: &str = "WebView2";

/// What the worker threads and the page ask of the UI thread.
enum UserEvent {
    Script(String),
    /// The user confirmed quitting during an OS send in one piece (`Command::Quit`).
    Quit,
}

/// Where a command of the page goes: most to the worker; a script error to stderr; "quit" to the window itself.
#[derive(Debug, PartialEq)]
enum Route {
    PageError(String),
    Quit,
    Worker(Command),
}

fn route(command: Command) -> Route {
    match command {
        Command::PageError(message) => Route::PageError(message),
        Command::Quit => Route::Quit,
        other => Route::Worker(other),
    }
}

/// The page's commands, handled on the UI thread: nothing here waits (the channel and the proxy only queue).
fn ipc_handler(
    commands: Sender<Command>,
    proxy: EventLoopProxy<UserEvent>,
    parse_errors: Emitter,
) -> impl Fn(Request<String>) + 'static {
    move |request: Request<String>| match command::parse(request.body()).map(route) {
        Ok(Route::PageError(message)) => eprintln!("tempester: page error: {message}"),
        Ok(Route::Quit) => {
            // The loop is gone only when the app is closing.
            let _ = proxy.send_event(UserEvent::Quit);
        }
        Ok(Route::Worker(parsed)) => {
            // The worker is gone only when the app is closing.
            let _ = commands.send(parsed);
        }
        Err(e) => parse_errors(Event::error("command", &AppError::Invalid(e.to_string()))),
    }
}

/// Stops the worker and ends the loop. `Worker::stop` returns at once even during an OS send: the send runs on its own
/// thread and the worker's shutdown never touches the port while it runs (`Machine::shutdown`).
fn quit(worker: &mut Option<Worker>, flow: &mut ControlFlow) {
    if let Some(w) = worker.take() {
        w.stop();
    }
    *flow = ControlFlow::Exit;
}

/// A close during an OS send is refused and told to the page: it asks the user to cancel first (a send in pieces) or
/// to confirm quitting now (a send in one piece, which has no cancel), then sends `quit`.
fn close_requested(worker: &mut Option<Worker>, emit: &Emitter, flow: &mut ControlFlow) {
    let updating = worker
        .as_ref()
        .is_some_and(|w| w.os_active.load(Ordering::SeqCst));
    if updating {
        emit(Event::new("close_blocked"));
    } else {
        quit(worker, flow);
    }
}

/// The address of the page: Windows serves custom protocols over http on `<name>.localhost`.
fn start_url() -> String {
    if cfg!(target_os = "windows") {
        format!("http://{SCHEME}.localhost/index.html")
    } else {
        format!("{SCHEME}://localhost/index.html")
    }
}

fn respond(request: &Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
    match assets::lookup(request.uri().path()) {
        Some((mime, body)) => Response::builder()
            .header(CONTENT_TYPE, mime)
            .body(body)
            .unwrap_or_else(|_| failure(StatusCode::INTERNAL_SERVER_ERROR)),
        None => failure(StatusCode::NOT_FOUND),
    }
}

fn failure(status: StatusCode) -> Response<Cow<'static, [u8]>> {
    let mut response = Response::new(Cow::Borrowed(&[][..]));
    *response.status_mut() = status;
    response
}

fn webview_data_dir(data_root: &Path) -> PathBuf {
    data_root.join(WEBVIEW_DATA_DIR)
}

/// A web context with a data folder, on Windows only. Given none, WebView2 keeps its profile beside the executable
/// (`tempester.exe.WebView2`): under Program Files, where the installer puts the app, that folder cannot be written,
/// the WebView2 environment fails and the window closes at once (the portable zip, unpacked somewhere writable, works).
/// WKWebView never reads the folder (wry's wkwebview backend ignores the context); WebKitGTK would move the data that
/// Linux users already have, for no defect there.
fn web_context(data_root: &Path) -> Option<WebContext> {
    cfg!(target_os = "windows").then(|| WebContext::new(Some(webview_data_dir(data_root))))
}

fn new_builder(context: Option<&mut WebContext>) -> WebViewBuilder<'_> {
    match context {
        Some(context) => WebViewBuilder::new_with_web_context(context),
        None => WebViewBuilder::new(),
    }
}

fn ui_error(error: impl std::fmt::Display) -> AppError {
    AppError::Io(error.to_string())
}

/// Opens the window and runs until it is closed.
pub fn run(connector: Box<dyn Connector>, storage: Storage) -> Result<(), AppError> {
    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    // The proxy is Send, not Sync: the mutex lets every worker thread share the emitter.
    let proxy = Mutex::new(event_loop.create_proxy());
    let emit: Emitter = Arc::new(move |event: Event| {
        let proxy = proxy.lock().unwrap_or_else(PoisonError::into_inner);
        // The loop is gone only when the app is closing.
        let _ = proxy.send_event(UserEvent::Script(script_for(&event)));
    });
    // Never dropped: `event_loop.run` does not return, and the web view may need its context as long as it lives.
    let mut context = web_context(storage.root());
    let worker = machine::spawn(connector, storage, Arc::clone(&emit))?;
    let on_command = ipc_handler(
        worker.commands.clone(),
        event_loop.create_proxy(),
        Arc::clone(&emit),
    );

    let window = WindowBuilder::new()
        .with_title("Tempester")
        .with_window_icon(window_icon::icon())
        .with_inner_size(LogicalSize::new(1320.0, 900.0))
        .with_min_inner_size(LogicalSize::new(900.0, 600.0))
        .build(&event_loop)
        .map_err(ui_error)?;
    let builder = new_builder(context.as_mut())
        .with_custom_protocol(SCHEME.to_string(), |_id, request| respond(&request))
        .with_url(start_url())
        .with_ipc_handler(on_command);
    let webview = build_webview(builder, &window)?;

    let mut worker = Some(worker);
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            WindowEvent::UserEvent(UserEvent::Script(js)) => {
                if let Err(e) = webview.evaluate_script(&js) {
                    eprintln!("tempester: cannot reach the page: {e}");
                }
            }
            WindowEvent::UserEvent(UserEvent::Quit) => quit(&mut worker, flow),
            WindowEvent::WindowEvent {
                event: Inner::CloseRequested,
                ..
            } => close_requested(&mut worker, &emit, flow),
            _ => {}
        }
    })
}

#[cfg(not(target_os = "linux"))]
fn build_webview(
    builder: WebViewBuilder,
    window: &tao::window::Window,
) -> Result<wry::WebView, AppError> {
    builder.build(window).map_err(ui_error)
}

#[cfg(target_os = "linux")]
fn build_webview(
    builder: WebViewBuilder,
    window: &tao::window::Window,
) -> Result<wry::WebView, AppError> {
    use tao::platform::unix::WindowExtUnix;
    use wry::WebViewBuilderExtUnix;
    let vbox = window
        .default_vbox()
        .ok_or_else(|| ui_error("no GTK container in the window"))?;
    builder.build_gtk(vbox).map_err(ui_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(path: &str) -> Response<Cow<'static, [u8]>> {
        let request = Request::builder()
            .uri(format!("{SCHEME}://localhost{path}"))
            .body(Vec::new())
            .unwrap();
        respond(&request)
    }

    #[test]
    fn test_route_when_quit_returns_the_window_and_every_other_command_the_worker() {
        assert_eq!(route(Command::Quit), Route::Quit);
        assert_eq!(
            route(Command::PageError("x".into())),
            Route::PageError("x".into())
        );
        assert_eq!(route(Command::OsCancel), Route::Worker(Command::OsCancel));
    }

    #[test]
    fn test_respond_when_known_asset_returns_200_with_its_media_type() {
        let response = get("/panel.js");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers()[CONTENT_TYPE],
            "text/javascript; charset=utf-8"
        );
        assert!(!response.body().is_empty());
    }

    #[test]
    fn test_respond_when_unknown_path_returns_404_with_an_empty_body() {
        let response = get("/secret");

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.body().is_empty());
    }

    #[test]
    fn test_respond_when_query_string_present_returns_the_asset_of_the_path() {
        assert_eq!(get("/app.css?v=1").status(), StatusCode::OK);
    }

    #[test]
    fn test_webview_data_dir_when_data_root_given_returns_its_webview2_folder() {
        let root = Path::new("/data/Tempester");

        assert_eq!(webview_data_dir(root), root.join("WebView2"));
    }

    #[test]
    fn test_web_context_when_built_returns_the_data_folder_on_windows_only() {
        let root = Path::new("/data/Tempester");

        let context = web_context(root);

        let folder = context.as_ref().and_then(WebContext::data_directory);
        let expected = cfg!(target_os = "windows").then(|| webview_data_dir(root));
        assert_eq!(folder, expected.as_deref());
    }
}
