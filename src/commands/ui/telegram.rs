use super::{write_header, write_string};
use crate::state::{TelegramLoginMode, TelegramProxy};

pub(crate) enum TelegramAction {
    Refresh,
    SetEnabled(bool),
    SetProxy(TelegramProxy),
    SetLoginMode(TelegramLoginMode),
    SetPhone(String),
    SetCode(String),
    SetPassword(String),
    SetEmail(String),
    SetEmailCode(String),
    Register { first_name: String, last_name: String },
    ResendCode,
    Logout,
}

impl TelegramAction {
    pub(crate) fn id(&self) -> u8 {
        match self {
            Self::Refresh => 37,
            Self::SetEnabled(_) => 38,
            Self::SetProxy(_) => 39,
            Self::SetLoginMode(_) => 40,
            Self::SetPhone(_) => 41,
            Self::SetCode(_) => 42,
            Self::SetPassword(_) => 43,
            Self::SetEmail(_) => 44,
            Self::SetEmailCode(_) => 45,
            Self::Register { .. } => 46,
            Self::ResendCode => 47,
            Self::Logout => 48,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        let fields: &[&str] = match self {
            Self::SetProxy(TelegramProxy::Socks5 {
                host, user, password, ..
            }) => &[host, user, password],
            Self::SetProxy(TelegramProxy::MtProto { host, secret, .. }) => &[host, secret],
            Self::SetPhone(s) | Self::SetCode(s) | Self::SetPassword(s) | Self::SetEmail(s) | Self::SetEmailCode(s) => {
                &[s]
            }
            Self::Register { first_name, last_name } => &[first_name, last_name],
            _ => &[],
        };
        if fields.iter().any(|s| s.len() > u16::MAX as usize) {
            return Err("Telegram text exceeds 65535 UTF-8 bytes");
        }
        Ok(())
    }

    pub(crate) fn build(&self, uid: u64) -> Vec<u8> {
        let mut out = Vec::new();
        write_header(&mut out, self.id(), uid);
        match self {
            Self::Refresh | Self::ResendCode | Self::Logout => {}
            Self::SetEnabled(value) => out.push(u8::from(*value)),
            Self::SetLoginMode(mode) => out.push(u8::from(*mode == TelegramLoginMode::Qr)),
            Self::SetPhone(s) | Self::SetCode(s) | Self::SetPassword(s) | Self::SetEmail(s) | Self::SetEmailCode(s) => {
                write_string(&mut out, s)
            }
            Self::Register { first_name, last_name } => {
                write_string(&mut out, first_name);
                write_string(&mut out, last_name);
            }
            Self::SetProxy(proxy) => {
                let (kind, host, port, user, password): (u8, &str, u16, &str, &str) = match proxy {
                    TelegramProxy::None => (0, "", 0, "", ""),
                    TelegramProxy::Socks5 {
                        host,
                        port,
                        user,
                        password,
                    } => (1, host, *port, user, password),
                    TelegramProxy::MtProto { host, port, secret } => (2, host, *port, "", secret),
                };
                out.push(kind);
                write_string(&mut out, host);
                out.extend_from_slice(&port.to_le_bytes());
                write_string(&mut out, user);
                write_string(&mut out, password);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::registry::{read_string, CURRENT_PROTO_CMD_VER};

    fn actions() -> Vec<TelegramAction> {
        vec![
            TelegramAction::Refresh,
            TelegramAction::SetEnabled(true),
            TelegramAction::SetProxy(TelegramProxy::Socks5 {
                host: "proxy.test".into(),
                port: 1080,
                user: "user".into(),
                password: "secret".into(),
            }),
            TelegramAction::SetLoginMode(TelegramLoginMode::Qr),
            TelegramAction::SetPhone("+10000000000".into()),
            TelegramAction::SetCode("12345".into()),
            TelegramAction::SetPassword("secret".into()),
            TelegramAction::SetEmail("test@example.test".into()),
            TelegramAction::SetEmailCode("123456".into()),
            TelegramAction::Register {
                first_name: "First".into(),
                last_name: "Last".into(),
            },
            TelegramAction::ResendCode,
            TelegramAction::Logout,
        ]
    }

    #[test]
    fn telegram_actions_encode_exact_delphi_headers_and_fields() {
        for (id, action) in (37..=48).zip(actions()) {
            action.validate().unwrap();
            let raw = action.build(0x1122334455667788);
            assert_eq!(raw[0], id);
            assert_eq!(&raw[1..3], &CURRENT_PROTO_CMD_VER.to_le_bytes());
            assert_eq!(&raw[3..11], &0x1122334455667788u64.to_le_bytes());
            let expected: &[u8] = match id {
                37 | 47 | 48 => b"",
                38 | 40 => b"\x01",
                39 => b"\x01\x0a\x00proxy.test\x38\x04\x04\x00user\x06\x00secret",
                41 => b"\x0c\x00+10000000000",
                42 => b"\x05\x0012345",
                43 => b"\x06\x00secret",
                44 => b"\x11\x00test@example.test",
                45 => b"\x06\x00123456",
                46 => b"\x05\x00First\x04\x00Last",
                _ => unreachable!(),
            };
            assert_eq!(&raw[11..], expected, "command {id}");
        }
        assert_eq!(&TelegramAction::SetEnabled(false).build(1)[11..], &[0]);
        assert_eq!(
            &TelegramAction::SetLoginMode(TelegramLoginMode::Phone).build(1)[11..],
            &[0]
        );
        assert_eq!(&TelegramAction::SetProxy(TelegramProxy::None).build(1)[11..], &[0; 9]);
        let mtproto = TelegramAction::SetProxy(TelegramProxy::MtProto {
            host: "host".into(),
            port: 443,
            secret: "test".into(),
        });
        assert_eq!(&mtproto.build(1)[11..], b"\x02\x04\x00host\xbb\x01\x00\x00\x04\x00test");
        let utf8 = "\u{0422}\u{0435}\u{0441}\u{0442}";
        let raw = TelegramAction::SetPassword(utf8.into()).build(1);
        assert_eq!(&raw[11..13], &8u16.to_le_bytes());
        assert_eq!(read_string(&raw, &mut 11).as_deref(), Some(utf8));
    }

    #[test]
    fn telegram_text_rejects_wire_truncation_without_echoing_secrets() {
        assert!(TelegramAction::SetPassword("x".repeat(65535)).validate().is_ok());
        let err = TelegramAction::SetPassword("x".repeat(65536)).validate().unwrap_err();
        assert_eq!(err, "Telegram text exceeds 65535 UTF-8 bytes");
        let proxy = TelegramProxy::MtProto {
            host: "test".into(),
            port: 443,
            secret: "SECRET_TOKEN".into(),
        };
        assert!(!format!("{proxy:?}").contains("SECRET_TOKEN"));
    }
}
