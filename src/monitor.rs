//! Main-context bridge to the single collector. eventfd wakes GTK only when a
//! sample arrives; there is no permanent GUI polling timer.
use gtk::glib;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    rc::Rc,
    sync::Arc,
    time::Duration,
};
use tend::sampler::{Detail, Hub, Subscription, Update};
type Callback = Rc<dyn Fn(Update)>;
struct Entry {
    subscription: Subscription,
    callback: Callback,
    once: bool,
}
struct Dispatcher {
    hub: Hub,
    fd: Arc<OwnedFd>,
    entries: RefCell<BTreeMap<u64, Entry>>,
    sequence: Cell<u64>,
    source: RefCell<Option<glib::SourceId>>,
}
thread_local! { static DISPATCHER: RefCell<Option<Rc<Dispatcher>>> = const { RefCell::new(None) }; }
pub struct Handle {
    dispatcher: Rc<Dispatcher>,
    id: u64,
}
impl Drop for Handle {
    fn drop(&mut self) {
        self.dispatcher.entries.borrow_mut().remove(&self.id);
    }
}
fn dispatcher() -> Result<Rc<Dispatcher>, String> {
    DISPATCHER.with(|cell| {
        if let Some(value) = cell.borrow().as_ref() {
            return Ok(value.clone());
        }
        let fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        let fd = Arc::new(unsafe { OwnedFd::from_raw_fd(fd) });
        let notifier = fd.clone();
        let hub = Hub::with_notifier(move || {
            let one = 1u64.to_ne_bytes();
            // EAGAIN means a wake is already pending; slots keep the latest data.
            unsafe {
                libc::write(notifier.as_raw_fd(), one.as_ptr().cast(), one.len());
            }
        });
        let value = Rc::new(Dispatcher {
            hub,
            fd,
            entries: RefCell::new(BTreeMap::new()),
            sequence: Cell::new(0),
            source: RefCell::new(None),
        });
        let weak = Rc::downgrade(&value);
        *value.source.borrow_mut() = Some(glib::source::unix_fd_add_local(
            value.fd.as_raw_fd(),
            glib::IOCondition::IN,
            move |fd, _| {
                let Some(value) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let mut counter = 0u64;
                unsafe {
                    libc::read(fd, (&mut counter as *mut u64).cast(), 8);
                }
                let mut callbacks = Vec::new();
                let mut finished = Vec::new();
                for (id, entry) in value.entries.borrow().iter() {
                    if let Some(update) = entry.subscription.take_latest() {
                        callbacks.push((entry.callback.clone(), update));
                        if entry.once {
                            finished.push(*id);
                        }
                    }
                }
                for id in finished {
                    value.entries.borrow_mut().remove(&id);
                }
                for (callback, update) in callbacks {
                    callback(update);
                }
                glib::ControlFlow::Continue
            },
        ));
        *cell.borrow_mut() = Some(value.clone());
        Ok(value)
    })
}
pub fn subscribe(
    detail: Detail,
    period: Duration,
    callback: impl Fn(Update) + 'static,
) -> Result<Handle, String> {
    let dispatcher = dispatcher()?;
    let id = dispatcher.sequence.get() + 1;
    dispatcher.sequence.set(id);
    let subscription = dispatcher.hub.subscribe(detail, period);
    dispatcher.entries.borrow_mut().insert(
        id,
        Entry {
            subscription,
            callback: Rc::new(callback),
            once: false,
        },
    );
    Ok(Handle { dispatcher, id })
}
pub fn request(detail: Detail, callback: impl Fn(Update) + 'static) -> Result<(), String> {
    let dispatcher = dispatcher()?;
    let id = dispatcher.sequence.get() + 1;
    dispatcher.sequence.set(id);
    let subscription = dispatcher.hub.request(detail);
    dispatcher.entries.borrow_mut().insert(
        id,
        Entry {
            subscription,
            callback: Rc::new(callback),
            once: true,
        },
    );
    Ok(())
}
pub fn shutdown() {
    DISPATCHER.with(|cell| {
        if let Some(value) = cell.borrow_mut().take() {
            value.entries.borrow_mut().clear();
            value.hub.shutdown();
            if let Some(source) = value.source.borrow_mut().take() {
                source.remove();
            }
        }
    });
}
