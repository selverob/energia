//! # DBus interface proxy for: `org.freedesktop.login1.Manager`
//!
//! This code was generated by `zbus-xmlgen` `1.0.0` from DBus introspection data.
//! Source: `Interface '/org/freedesktop/login1' from service 'org.freedesktop.login1' on system bus`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://zeenix.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This DBus object implements
//! [standard DBus interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::PeerProxy`]
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PropertiesProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

use zbus::dbus_proxy;
use zvariant;

#[dbus_proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
pub trait Manager {
    /// GetSession method
    fn get_session(&self, session_id: &str) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// ListSessions method
    fn list_sessions(
        &self,
    ) -> zbus::Result<Vec<(String, u32, String, String, zvariant::OwnedObjectPath)>>;

    /// IdleHint property
    #[dbus_proxy(property)]
    fn idle_hint(&self) -> zbus::Result<bool>;

    /// IdleSinceHint property
    #[dbus_proxy(property)]
    fn idle_since_hint(&self) -> zbus::Result<u64>;

    /// IdleSinceHintMonotonic property
    #[dbus_proxy(property)]
    fn idle_since_hint_monotonic(&self) -> zbus::Result<u64>;
}