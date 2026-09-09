# Telegram On The Core

`client.telegram()` manages the core's built-in Telegram reader: enable/disable,
phone or QR login, verification codes, two-factor password, registration, proxy
and logout. Messages continue through the core's existing Telegram processing;
this API is not a Telegram chat/history client.

The Telegram account and active proxy are shared by MoonBot processes on the
core's machine. Enabling Telegram and saving proxy settings affect the selected
core; applying a proxy or logging out can affect the other bots using that service.

## State And Actions

```rust
use moonproto::Event;
use moonproto::state::{SettingsEvent, TelegramLoginMode};

client.telegram().refresh()?; // Passive: no new login, reset or QR rotation.

// Inside the terminal's existing event loop:
for event in client.drain_events() {
    if matches!(event, Event::Settings(SettingsEvent::TelegramUpdated)) {
        if let Some(state) = client.telegram().state() {
            // Replace the displayed state, including absent fields.
            if let Some(service) = &state.service {
                match service.auth_state.as_str() {
                    "wait_phone" => { /* Show phone / QR choice. */ }
                    "wait_qr_confirmation" => { /* Render details.qr_link as QR. */ }
                    "ready" => { /* Logged in; display connection separately. */ }
                    _ => { /* See the step table below. */ }
                }
            }
        }
    }
}

// Only on an explicit user choice, at wait_phone:
client.telegram().set_login_mode(TelegramLoginMode::Qr)?;
```

The same immutable `Arc<TelegramState>` is in `snapshot.settings().telegram`.
It starts as `None`. The core pushes a full snapshot on connect/rebind and when
state changes; `refresh()` requests one even if unchanged. Receiving Telegram
state does not block `Ready`. Actions use the normal runtime queue and wait for
Init to complete if called early.

Each snapshot replaces the previous one. Missing optional fields mean unavailable;
missing booleans mean false. A successful API call means **queued**, not completed.
The snapshot immediately following an action may still show the old step because
the core and service work asynchronously. Only `service.auth_state == "ready"`
proves Telegram authentication. Do not treat MoonProto `Ready` as Telegram login.

When opening the panel, call `refresh()`. On connection loss, mark the displayed
state stale, disable account actions and hide QR. Retained snapshots can still
contain old data. After `LifecycleEvent::Connected { fresh: false }`, request
state again and use the next `TelegramUpdated`. Never automatically repeat phone,
QR, code, reset or logout actions on reconnect. If an expected change does not
arrive, refresh first and let the user retry. There are no per-action result tickets.

## Panel Availability

- `enabled`: saved setting on this core. `set_enabled(false)` does not log out.
- `service_online`: connection from the core to its local service, not to Telegram.
- `state_supported`: the service supports the full remote-login state. If false,
  show that remote login is unavailable; do not infer an input step.
- `service_version`: responding service version. Full state requires the updated
  MoonTelegramService, implemented in 7.67.0.11. Use `state_supported` as the gate,
  not a hard-coded version comparison.
- `client_state`, `setup_error`, `client_error`: core/service status, installation
  failure or login-reset failure. Display non-empty errors. Enabling uses the
  core's existing installation path and process permissions.

Allow enabling and saving proxy settings while disabled. Show authentication
controls only with a live core connection, enabled service, `service_online`,
`state_supported` and a received `service` snapshot.

## Login Steps

All types below are exported from `moonproto::state`. `service.auth_state` and
`connection` are strings so future states remain readable instead of failing parsing.

| Auth state | Display / user action |
|---|---|
| `wait_phone` | Phone in international format -> `set_phone(...)`; QR -> `set_login_mode(TelegramLoginMode::Qr)`. |
| `wait_qr_confirmation` | Encode **all** of `details.qr_link` as QR. It is a `tg://login?token=...` value, not an image URL. |
| `wait_code` | `details.phone`, `code_type`, `next_code_type`, `resend_at`; submit with `set_code(...)`. |
| `wait_password` | `details.password_hint`, `recovery_email_pattern`; submit 2FA password with `set_password(...)`. |
| `wait_email_address` | Submit with `set_email(...)`. |
| `wait_email_code` | `details.email_pattern`, `code_length`; submit with `set_email_code(...)`. |
| `wait_registration` | Show `details.terms`, `min_user_age`, `show_popup`; call `register(first_name, last_name)` only after explicit acceptance. |
| `ready` | Logged in. `service.phone` is the actual account phone, also after QR login. |
| `wait_premium_purchase` | An external Telegram step is required; show `details.support_email`, `support_subject`, and allow cancelling login. |
| `starting`, `wait_tdlib_parameters`, `logging_out`, `closing`, `closed` | Transitional; wait for state updates. |
| Unknown | Show an unsupported step and offer refresh. Do not guess which input to submit. |

`TelegramLoginMode::Phone` cancels an unfinished login and returns to phone entry.
To switch from code/password/email to QR, choose Phone, wait for `wait_phone`, then
choose Qr. Re-selecting Qr while QR is already shown does not reset it. Selecting
a mode while `ready` does not log out.

Redraw QR when its link changes. Remove it when the step changes or the core/service
disconnects. TDLib owns token renewal. QR links, codes, passwords and proxy secrets
must not enter application logs, screenshots collected as diagnostics, or storage.
The main state types omit auth details from `Debug` output.

`TelegramCodeType.kind` can be `telegram`, `sms`, `sms_word`, `sms_phrase`, `call`,
`flash_call`, `missed_call`, `fragment` or `unsupported`. Depending on the kind,
use `length`, `first_letter`, `first_word`, `pattern`, `prefix` or `url` to explain
delivery. Missing `code_type`/`next_code_type` is not an available method.

For phone-code resend, enable `resend_code()` only when `next_code_type` exists
and `resend_at` (Unix seconds UTC) has passed. It also works at `wait_email_code`;
TDLib enforces its resend limits. `service.error` contains `{code, message}`:
an incorrect code/password can leave the same step active for another attempt.
Render the error even when `auth_state` did not change.

`service.connection` is independent: `unknown`, `waiting_for_network`,
`connecting_to_proxy`, `connecting`, `updating`, `ready`, or a future value.
A logged-in account can temporarily have no network connection.

## Proxy And Logout

```rust
use moonproto::state::TelegramProxy;
client.telegram().set_proxy(TelegramProxy::Socks5 { host, port, user, password })?;
```

Other choices: `TelegramProxy::None` and
`TelegramProxy::MtProto { host, port, secret }`. This is a **complete** update;
an empty password clears it, not "keep the old password". Saved fields are
`proxy_type`, `proxy_host`, `proxy_port`, `proxy_user`, `proxy_password_set`.
The password/secret itself is never returned, so an edit must collect it again.

`service.proxy` reports the actually enabled proxy, or is absent until known;
`mode == "none"` means no proxy. It can differ from saved settings after a failed
apply or another bot's change. Show `service.proxy_error` beside this actual state.

`logout()` logs out and resets the shared local Telegram account data on the
core's machine. Require explicit confirmation explaining the effect on other bots.
Use `set_enabled(false)` to stop only this core's reader without logging everyone out.

## FireTest

`fire_test_telegram_state` checks initial snapshot delivery, passive refresh and
a second connection through the public API. It does not enable the service,
change proxy, log in or log out. It reports unavailable remote login explicitly.
Interactive phone/QR/2FA validation requires a consenting user and a test account.
