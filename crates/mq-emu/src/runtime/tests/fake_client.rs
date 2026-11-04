//! A stand-in for eqclient.dll: [`shows`] is the client showing a message.

use std::cell::RefCell;

use eq_client::abi::{API_VERSION, Api, Status};

#[derive(Default)]
struct Client {
    /// Why eqclient.dll "isn't there", if it isn't.
    missing: Option<String>,
    /// What watching returns; hooked unless set.
    status: Option<Status>,
    watched: Vec<u32>,
    /// Every watched message shown, in order; cursors index it.
    shown: Vec<u32>,
    /// Messages lost to the next poll.
    lose: u64,
}

thread_local! {
    static CLIENT: RefCell<Client> = RefCell::new(Client::default());
}

static API: Api = Api { version: API_VERSION, watch_string_ids, string_ids_head, poll_string_ids };

pub fn client() -> Result<&'static Api, String> {
    CLIENT.with(|c| c.borrow().missing.clone()).map_or(Ok(&API), Err)
}

/// The client shows message `id`; recorded only if watched, as the DLL does.
pub fn shows(id: u32) {
    CLIENT.with(|c| {
        let mut c = c.borrow_mut();
        if c.watched.contains(&id) {
            c.shown.push(id);
        }
    });
}

pub fn set_missing(why: &str) {
    CLIENT.with(|c| c.borrow_mut().missing = Some(why.to_owned()));
}

pub fn set_status(status: Status) {
    CLIENT.with(|c| c.borrow_mut().status = Some(status));
}

/// The next poll reports `n` messages lost.
pub fn lose(n: u64) {
    CLIENT.with(|c| c.borrow_mut().lose = n);
}

pub fn watched() -> Vec<u32> {
    CLIENT.with(|c| c.borrow().watched.clone())
}

unsafe extern "C" fn watch_string_ids(ids: *const u32, count: usize) -> Status {
    CLIENT.with(|c| {
        let mut c = c.borrow_mut();
        let status = c.status.unwrap_or(Status::HOOKED);
        if status.is_hooked() {
            c.watched.extend_from_slice(unsafe { std::slice::from_raw_parts(ids, count) });
        }
        status
    })
}

unsafe extern "C" fn string_ids_head() -> u64 {
    CLIENT.with(|c| c.borrow().shown.len() as u64)
}

unsafe extern "C" fn poll_string_ids(cursor: *mut u64, out: *mut u32, capacity: usize, missed: *mut u64) -> usize {
    CLIENT.with(|c| {
        let mut c = c.borrow_mut();
        let cursor = unsafe { &mut *cursor };
        let new = &c.shown[*cursor as usize..];
        let n = new.len().min(capacity);
        unsafe { std::ptr::copy_nonoverlapping(new.as_ptr(), out, n) };
        *cursor += n as u64;
        unsafe { *missed += std::mem::take(&mut c.lose) };
        n
    })
}
