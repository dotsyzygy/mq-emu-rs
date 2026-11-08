//! eqclient.dll: eq-client's hooks, owned once per game process and shared by
//! every plugin through [`eq_client::abi::Api`]. All its logic is in
//! `eq_client::host`, where it's tested; this only exports it.

use eq_client::abi::{Api, DllVersion};

/// The API table. The first call pins the DLL, so it's never unloaded while
/// the client may call its hooks.
#[unsafe(no_mangle)]
pub extern "C" fn eqclient_api() -> *const Api {
    static PINNED: std::sync::Once = std::sync::Once::new();
    PINNED.call_once(|| {
        eq_client::host::pin_self();
    });
    &eq_client::host::API
}

/// This DLL's version, as data `cargo mq deploy` reads from the file
/// ([`eq_client::abi::VERSION_EXPORT`]).
#[unsafe(no_mangle)]
#[allow(non_upper_case_globals)]
pub static eqclient_version: DllVersion = DllVersion::CURRENT;
