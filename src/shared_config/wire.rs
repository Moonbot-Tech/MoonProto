//! Wire-format parse and serialize for the shared-config binary payload.
//!
//! The payload is the inner (pre-gzip, pre-base64) binary stream.  Outer
//! wrappers (gzip, base64, clipboard) are in [`super::clipboard`].

use super::sections::*;
use std::fmt;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Parse / serialize errors for shared-config payloads.
#[derive(Debug, Clone)]
pub struct SharedConfigError {
    pub(super) msg: String,
}

impl SharedConfigError {
    pub(super) fn new(s: impl Into<String>) -> Self {
        Self { msg: s.into() }
    }
}

impl fmt::Display for SharedConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}

impl std::error::Error for SharedConfigError {}

impl From<&str> for SharedConfigError {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl From<String> for SharedConfigError {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

fn err<T>(msg: &str) -> Result<T, SharedConfigError> {
    Err(SharedConfigError::new(msg))
}

fn copy_bytes(data: &[u8]) -> Result<Vec<u8>, SharedConfigError> {
    let mut out = Vec::new();
    out.try_reserve(data.len())
        .map_err(|err| SharedConfigError::new(format!("shared config allocation failed: {err}")))?;
    out.extend_from_slice(data);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Production safe-share format constants
// ---------------------------------------------------------------------------

const MAGIC: &[u8; 4] = b"MBSP";
const VERSION: u8 = 7;
const HEADER_SIZE: usize = 7; // 4 magic + 1 version + 2 config_version
const BLOCK_HEADER_SIZE: usize = 5; // 1 kind + 4 size

const KIND_SIGNALS: u8 = 1;
const KIND_TRADING: u8 = 2;
const KIND_VISUAL: u8 = 3;
const KIND_THEME: u8 = 4;
const KIND_INI: u8 = 5;
const KIND_UI: u8 = 6;

const MAX_STRING_LEN: i32 = 2 * 1024 * 1024;
const MAX_LIST_COUNT: i32 = 10_000;
const MAX_INI_ENTRIES: i32 = 2048;
pub(super) const MAX_BLOCK_SIZE: u32 = 16 * 1024 * 1024;
pub(super) const MAX_PAYLOAD_SIZE: usize =
    HEADER_SIZE + 6 * (BLOCK_HEADER_SIZE + MAX_BLOCK_SIZE as usize);

// Required section mask: bits 1..6 set.
const REQUIRED_MASK: u32 = (1 << KIND_SIGNALS)
    | (1 << KIND_TRADING)
    | (1 << KIND_VISUAL)
    | (1 << KIND_THEME)
    | (1 << KIND_INI)
    | (1 << KIND_UI);

// ---------------------------------------------------------------------------
// Low-level reader helpers (slice + position)
// ---------------------------------------------------------------------------

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn need(&self, n: usize) -> Result<(), SharedConfigError> {
        if self
            .pos
            .checked_add(n)
            .is_none_or(|end| end > self.data.len())
        {
            return err("unexpected end of shared config data");
        }
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, SharedConfigError> {
        self.need(1)?;
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_bool(&mut self) -> Result<bool, SharedConfigError> {
        Ok(self.read_u8()? != 0)
    }

    fn read_u16(&mut self) -> Result<u16, SharedConfigError> {
        self.need(2)?;
        let v = u16::from_le_bytes(self.data[self.pos..self.pos + 2].try_into().unwrap());
        self.pos += 2;
        Ok(v)
    }

    fn read_i32(&mut self) -> Result<i32, SharedConfigError> {
        self.need(4)?;
        let v = i32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    fn read_u32(&mut self) -> Result<u32, SharedConfigError> {
        self.need(4)?;
        let v = u32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    fn read_f32(&mut self) -> Result<f32, SharedConfigError> {
        self.need(4)?;
        let v = f32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    fn read_f64(&mut self) -> Result<f64, SharedConfigError> {
        self.need(8)?;
        let v = f64::from_le_bytes(self.data[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        Ok(v)
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8], SharedConfigError> {
        self.need(n)?;
        let sl = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(sl)
    }

    /// Wire `StringX`: i32 UTF-16 code-unit count + UTF-16LE bytes.
    fn read_string_x(&mut self) -> Result<String, SharedConfigError> {
        let len = self.read_i32()?;
        if !(0..=MAX_STRING_LEN).contains(&len) {
            return err("wrong shared config string length");
        }
        let byte_len = (len as usize) * 2;
        let raw = self.read_bytes(byte_len)?;
        let mut out = String::new();
        out.try_reserve((len as usize).saturating_mul(3))
            .map_err(|err| {
                SharedConfigError::new(format!("shared config allocation failed: {err}"))
            })?;
        for decoded in char::decode_utf16(
            raw.chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]])),
        ) {
            out.push(decoded.unwrap_or(char::REPLACEMENT_CHARACTER));
        }
        Ok(out)
    }

    /// String-list wire shape: i32 count followed by encoded strings.
    fn read_string_list_x(&mut self) -> Result<Vec<String>, SharedConfigError> {
        let cnt = self.read_i32()?;
        if !(0..=MAX_LIST_COUNT).contains(&cnt) {
            return err("wrong shared config list count");
        }
        let mut v = Vec::new();
        v.try_reserve(cnt as usize).map_err(|err| {
            SharedConfigError::new(format!("shared config allocation failed: {err}"))
        })?;
        for _ in 0..cnt {
            v.push(self.read_string_x()?);
        }
        Ok(v)
    }

    fn read_bool_array<const N: usize>(&mut self) -> Result<[bool; N], SharedConfigError> {
        let raw = self.read_bytes(N)?;
        let mut out = [false; N];
        for (i, &b) in raw.iter().enumerate() {
            out[i] = b != 0;
        }
        Ok(out)
    }

    fn read_u8_array<const N: usize>(&mut self) -> Result<[u8; N], SharedConfigError> {
        let raw = self.read_bytes(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(raw);
        Ok(out)
    }

    fn read_u16_array<const N: usize>(&mut self) -> Result<[u16; N], SharedConfigError> {
        let raw = self.read_bytes(N * 2)?;
        let mut out = [0u16; N];
        for (i, chunk) in raw.chunks_exact(2).enumerate() {
            out[i] = u16::from_le_bytes([chunk[0], chunk[1]]);
        }
        Ok(out)
    }

    fn read_u32_array<const N: usize>(&mut self) -> Result<[u32; N], SharedConfigError> {
        let raw = self.read_bytes(N * 4)?;
        let mut out = [0u32; N];
        for (i, chunk) in raw.chunks_exact(4).enumerate() {
            out[i] = u32::from_le_bytes(chunk.try_into().unwrap());
        }
        Ok(out)
    }

    fn read_f32_array<const N: usize>(&mut self) -> Result<[f32; N], SharedConfigError> {
        let raw = self.read_bytes(N * 4)?;
        let mut out = [0.0f32; N];
        for (i, chunk) in raw.chunks_exact(4).enumerate() {
            out[i] = f32::from_le_bytes(chunk.try_into().unwrap());
        }
        Ok(out)
    }

    fn read_f64_array<const N: usize>(&mut self) -> Result<[f64; N], SharedConfigError> {
        let raw = self.read_bytes(N * 8)?;
        let mut out = [0.0f64; N];
        for (i, chunk) in raw.chunks_exact(8).enumerate() {
            out[i] = f64::from_le_bytes(chunk.try_into().unwrap());
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Low-level writer helpers
// ---------------------------------------------------------------------------

struct Writer {
    buf: Vec<u8>,
    active_block_body_start: Option<usize>,
    error: Option<SharedConfigError>,
}

impl Writer {
    fn new() -> Self {
        Self {
            buf: Vec::with_capacity(4096),
            active_block_body_start: None,
            error: None,
        }
    }

    fn fail(&mut self, message: impl Into<String>) {
        if self.error.is_none() {
            self.error = Some(SharedConfigError::new(message));
        }
    }

    fn reserve_for(&mut self, additional: usize) -> bool {
        if self.error.is_some() {
            return false;
        }
        let Some(next_len) = self.buf.len().checked_add(additional) else {
            self.fail("shared config payload size overflow");
            return false;
        };
        if next_len > MAX_PAYLOAD_SIZE {
            self.fail("shared config payload too large");
            return false;
        }
        if self
            .active_block_body_start
            .is_some_and(|start| next_len.saturating_sub(start) > MAX_BLOCK_SIZE as usize)
        {
            self.fail("shared config block too large");
            return false;
        }
        if let Err(err) = self.buf.try_reserve(additional) {
            self.fail(format!("shared config allocation failed: {err}"));
            return false;
        }
        true
    }

    fn write_u8(&mut self, v: u8) {
        if self.reserve_for(1) {
            self.buf.push(v);
        }
    }
    fn write_bool(&mut self, v: bool) {
        self.write_u8(v as u8);
    }
    fn write_u16(&mut self, v: u16) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_i32(&mut self, v: i32) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_u32(&mut self, v: u32) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_f32(&mut self, v: f32) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_f64(&mut self, v: f64) {
        self.write_bytes(&v.to_le_bytes());
    }
    fn write_bytes(&mut self, b: &[u8]) {
        if self.reserve_for(b.len()) {
            self.buf.extend_from_slice(b);
        }
    }

    /// Wire `StringX`: i32 UTF-16 code-unit count + UTF-16LE bytes.
    fn write_string_x(&mut self, s: &str) {
        let len = s.encode_utf16().count();
        if len > MAX_STRING_LEN as usize {
            self.fail("shared config string too long");
            return;
        }
        self.write_i32(len as i32);
        let Some(byte_len) = len.checked_mul(2) else {
            self.fail("shared config string size overflow");
            return;
        };
        if !self.reserve_for(byte_len) {
            return;
        }
        for c in s.encode_utf16() {
            self.buf.extend_from_slice(&c.to_le_bytes());
        }
    }

    fn write_string_list_x(&mut self, list: &[String]) {
        if list.len() > MAX_LIST_COUNT as usize {
            self.fail("shared config list too long");
            return;
        }
        self.write_i32(list.len() as i32);
        for s in list {
            self.write_string_x(s);
        }
    }

    fn write_bool_array(&mut self, arr: &[bool]) {
        for &b in arr {
            self.write_bool(b);
        }
    }

    fn write_u8_array(&mut self, arr: &[u8]) {
        self.write_bytes(arr);
    }

    fn write_u16_array(&mut self, arr: &[u16]) {
        for &v in arr {
            self.write_u16(v);
        }
    }

    fn write_u32_array(&mut self, arr: &[u32]) {
        for &v in arr {
            self.write_u32(v);
        }
    }

    fn write_f32_array(&mut self, arr: &[f32]) {
        for &v in arr {
            self.write_f32(v);
        }
    }

    fn write_f64_array(&mut self, arr: &[f64]) {
        for &v in arr {
            self.write_f64(v);
        }
    }

    fn finish(self) -> Result<Vec<u8>, SharedConfigError> {
        match self.error {
            Some(err) => Err(err),
            None => Ok(self.buf),
        }
    }
}

// ---------------------------------------------------------------------------
// Block-level helpers
// ---------------------------------------------------------------------------

struct BlockInfo {
    body_start: usize,
    block_end: usize,
}

fn read_block_header(r: &mut Reader) -> Result<Option<(u8, BlockInfo)>, SharedConfigError> {
    if r.remaining() == 0 {
        return Ok(None);
    }
    if r.remaining() < BLOCK_HEADER_SIZE {
        return err("truncated shared config block header");
    }
    let kind = r.read_u8()?;
    let size = r.read_u32()?;
    if size > MAX_BLOCK_SIZE {
        return err("shared config block too large");
    }
    let body_start = r.pos;
    let block_end = body_start
        .checked_add(size as usize)
        .ok_or_else(|| SharedConfigError::new("shared-config block size overflow"))?;
    if block_end > r.data.len() {
        return err("shared config block extends past end");
    }
    Ok(Some((
        kind,
        BlockInfo {
            body_start,
            block_end,
        },
    )))
}

fn begin_block(w: &mut Writer, kind: u8) -> usize {
    w.write_u8(kind);
    let size_pos = w.buf.len();
    w.write_u32(0); // placeholder
    if w.error.is_none() {
        w.active_block_body_start = Some(size_pos + 4);
    }
    size_pos
}

fn end_block(w: &mut Writer, size_pos: usize) {
    w.active_block_body_start = None;
    if w.error.is_some() {
        return;
    }
    let body_start = size_pos + 4;
    let body_size = w.buf.len() - body_start;
    if body_size > MAX_BLOCK_SIZE as usize {
        w.fail("shared config block too large");
        return;
    }
    let body_size = body_size as u32;
    w.buf[size_pos..size_pos + 4].copy_from_slice(&body_size.to_le_bytes());
}

// ---------------------------------------------------------------------------
// Section parsers
// ---------------------------------------------------------------------------

fn parse_signal_config(r: &mut Reader) -> Result<SignalConfig, SharedConfigError> {
    Ok(SignalConfig {
        ver: r.read_i32()?,
        use_keywords: r.read_bool()?,
        use_token_tags: r.read_bool()?,
        buy_key_dist: r.read_i32()?,
        only_1_token: r.read_bool()?,
        buy_if_price_found: r.read_bool()?,
        x_found_price: r.read_i32()?,
        use_black_words: r.read_bool()?,
        words_count: r.read_i32()?,
        use_words_count: r.read_bool()?,
        use_price: r.read_bool()?,
        use_stops: r.read_bool()?,
        tokens_no_tags: r.read_bool()?,
        token_links: r.read_bool()?,
        use_lower_price_words: r.read_bool()?,
        x_lower_price: r.read_i32()?,
        special_formats: r.read_bool()?,
    })
}

fn parse_signals(r: &mut Reader, block_end: usize) -> Result<SignalsSection, SharedConfigError> {
    let ver = r.read_u8()?;
    if ver != 2 {
        return err("wrong shared signals version");
    }

    let pump_channel = r.read_string_x()?;
    let pump_channels = r.read_string_list_x()?;

    let mut s = SignalsSection {
        pump_channel,
        pump_channels,
        do_monitoring: r.read_bool()?,
        look_full_link_tlg: r.read_bool()?,
        multi_channels: r.read_bool()?,
        telegram_auto_buy: r.read_bool()?,
        load_deep_history: r.read_bool()?,
        only_secret_signal: r.read_bool()?,
        more_then_1_channel: r.read_bool()?,
        clipboard_auto_buy: r.read_bool()?,
        autodetect_auto_buy: r.read_bool()?,
        signal_sound: r.read_i32()?,
        play_signal_sound: r.read_bool()?,
        lower_case_token_tlg: r.read_bool()?,
        show_cross: r.read_bool()?,
        monitor_clipboard: r.read_bool()?,
        lower_case_token_cbd: r.read_bool()?,
        look_full_link_cbd: r.read_bool()?,
        msg_keywords_long: r.read_string_x()?,
        msg_keywords_short: r.read_string_x()?,
        msg_token_tags: r.read_string_x()?,
        msg_black_words: r.read_string_x()?,
        signal_config: parse_signal_config(r)?,
        advanced_filter: r.read_bool()?,
        auto_show_on_signal: r.read_bool()?,
        advanced_filter_clipboard: r.read_bool()?,
        base_vol_ind_type: r.read_i32()?,
        max_too_long_msg_len: r.read_i32()?,
        signal_sound_2: r.read_i32()?,
        sell_alert_level: r.read_i32()?,
        play_sell_alert: r.read_bool()?,
        last_used_order_type: r.read_i32()?,
        full_screen_prevent_signals: r.read_bool()?,
        listen_moon_channel: r.read_bool()?,
        lower_price_words: r.read_string_x()?,
        dont_buy_reply: r.read_bool()?,
        base_vol_n: r.read_i32()?,
        use_last_detect_caption: r.read_bool()?,
        buy_signal_sound: r.read_i32()?,
        play_buy_alert: r.read_bool()?,
        buy_alert_level: r.read_i32()?,
        strats_filter: r.read_string_x()?,
        news_tags_filter: r.read_string_x()?,
        news_tokens_filter: r.read_string_x()?,
        unknown_tail: Vec::new(),
    };

    // Capture any trailing bytes from a newer version.
    if r.pos < block_end {
        s.unknown_tail = copy_bytes(&r.data[r.pos..block_end])?;
    }
    Ok(s)
}

fn parse_auto_start(r: &mut Reader) -> Result<AutoStartConfig, SharedConfigError> {
    Ok(AutoStartConfig {
        auto_start: r.read_bool()?,
        auto_detect_on: r.read_bool()?,
        strategies_on: r.read_bool()?,
        work_time: r.read_bool()?,
        auto_stop_if_loss: r.read_bool()?,
        remember_state: r.read_bool()?,
        sell_if_loss: r.read_bool()?,
        dont_wait_sells: r.read_bool()?,
        auto_stop_loss: r.read_f64()?,
        panic_btc: r.read_bool()?,
        panic_market: r.read_bool()?,
        auto_stop_if_loss_hours: r.read_bool()?,
        auto_update: r.read_bool()?,
        restart_after_err: r.read_bool()?,
        restart_after_ping: r.read_bool()?,
        ignore_emulator: r.read_bool()?,
        stop_trades: r.read_i32()?,
        restart_err_time: r.read_i32()?,
        panic_btc_delta: r.read_f64()?,
        panic_market_delta: r.read_f64()?,
        auto_stop_on_errors: r.read_bool()?,
        auto_stop_on_ping: r.read_bool()?,
        sell_all_on_errors: r.read_bool()?,
        sell_all_on_ping: r.read_bool()?,
        errors_level: r.read_i32()?,
        ping_level: r.read_i32()?,
        restart_ping_time: r.read_i32()?,
        auto_stop_hours_val: r.read_f64()?,
        stop_hours: r.read_i32()?,
        stop_hours_trades: r.read_i32()?,
        panic_btc_delta_up: r.read_f64()?,
        work_time_from: r.read_f64()?,
        work_time_to: r.read_f64()?,
    })
}

fn parse_auto_start_2(r: &mut Reader) -> Result<AutoStartConfig2, SharedConfigError> {
    Ok(AutoStartConfig2 {
        restart_on_market: r.read_bool()?,
        btc_higher_then: r.read_f64()?,
        btc_lower_then: r.read_f64()?,
        market_higher_then: r.read_f64()?,
        show_old_listing: r.read_bool()?,
        reset_session: r.read_bool()?,
        max_session_cap: r.read_i32()?,
        rs_hours: r.read_i32()?,
    })
}

fn parse_multi_orders(r: &mut Reader) -> Result<MultiOrdersConfig, SharedConfigError> {
    Ok(MultiOrdersConfig {
        ver: r.read_u8()?,
        use_multi_orders: r.read_bool()?,
        buy_set_click: r.read_u8()?,
        buy_move_click: r.read_u8()?,
        sell_move_click: r.read_u8()?,
        replace_buy_kind: r.read_u8()?,
        replace_sell_kind: r.read_u8()?,
        split_sells: r.read_i32()?,
        show_orders_num: r.read_bool()?,
        kir_style: r.read_bool()?,
        fix_pos: r.read_u8()?,
        join_sell_kind: r.read_u8()?,
        short_set_click: r.read_u8()?,
        pending_short_set_click: r.read_u8()?,
        done_opacity: r.read_f32()?,
        buy_move_click_2: r.read_u8()?,
        sell_move_click_2: r.read_u8()?,
        replace_buy_kind_2: r.read_u8()?,
        replace_sell_kind_2: r.read_u8()?,
        same_hotkeys_for_move: r.read_bool()?,
        short_buy_move_click: r.read_u8()?,
        short_sell_move_click: r.read_u8()?,
        short_buy_move_click_2: r.read_u8()?,
        short_sell_move_click_2: r.read_u8()?,
    })
}

fn parse_report_config(r: &mut Reader) -> Result<ReportConfig, SharedConfigError> {
    Ok(ReportConfig {
        fields_vis: r.read_bool_array::<51>()?,
        fields_width: r.read_u16_array::<51>()?,
        only_active: r.read_bool()?,
        range: r.read_i32()?,
        filter1: r.read_u8_array::<26>()?,
        filter2: r.read_u8_array::<26>()?,
        filter3: r.read_u8_array::<26>()?,
        active_type: r.read_u8()?,
        rep_emulator_orders: r.read_bool()?,
        stretch: r.read_bool()?,
        only_2_orders: r.read_bool()?,
        use_leverage: r.read_bool()?,
        max_lines: r.read_u16()?,
        pos_direction: r.read_u8()?,
        store_liq: r.read_bool()?,
        store_fund: r.read_bool()?,
        by_close_date: r.read_bool()?,
    })
}

fn parse_arb_view(r: &mut Reader) -> Result<ArbViewConfig, SharedConfigError> {
    let ver = r.read_u8()?;
    if ver != 1 {
        return err("wrong shared arb settings version");
    }
    Ok(ArbViewConfig {
        ver,
        show_absolute: r.read_bool()?,
        show_numbers: r.read_bool()?,
        show_lines: r.read_bool()?,
        show_percent: r.read_bool()?,
        show_right: r.read_bool()?,
    })
}

fn parse_trading(r: &mut Reader, block_end: usize) -> Result<TradingSection, SharedConfigError> {
    let ver = r.read_u8()?;
    if ver != 3 {
        return err("wrong shared trading version");
    }

    let auto_delete_logs = r.read_i32()?;
    let log_level = r.read_i32()?;
    let binance_connection = r.read_i32()?;
    let auto_start = parse_auto_start(r)?;
    let auto_start_2 = parse_auto_start_2(r)?;
    let fav_markets = r.read_string_x()?;

    let x_ask = r.read_i32()?;
    let x_sell = r.read_i32()?;
    let x_sell_scalp = r.read_i32()?;
    let play_with = r.read_i32()?;
    let check_glass = r.read_bool()?;
    let max_orders = r.read_i32()?;
    let auto_sell_partial = r.read_i32()?;
    let price_range = r.read_i32()?;
    let sell_auto_move = r.read_bool()?;
    let pump_q_level = r.read_i32()?;
    let sell_x2_level = r.read_i32()?;
    let cancel_buy_on_sell_fill = r.read_bool()?;
    let auto_cancel_buy_order = r.read_i32()?;
    let vol_drop_level = r.read_i32()?;
    let price_drop_level = r.read_f32()?;
    let panic_if_vol_drop = r.read_bool()?;
    let panic_if_price_drop = r.read_bool()?;
    let manual_mode = r.read_bool()?;
    let draw_all_buy = r.read_bool()?;
    let trailing_stop = r.read_bool()?;
    let trailing_drop = r.read_f32()?;
    let engine_check_buy = r.read_bool()?;
    let use_manual_balance = r.read_bool()?;
    let fixed_trade_balance = r.read_f64()?;
    let use_current_ask = r.read_bool()?;
    let g_take_profit = r.read_f64()?;
    let use_g_take_profit = r.read_bool()?;
    let show_order_comment = r.read_bool()?;
    let chart_clean_up_time = r.read_i32()?;

    // MoonBotConfig (4 fields)
    let moonbot_config = MoonBotConfig {
        silent: r.read_bool()?,
        auto_margin_borrow: r.read_bool()?,
        auto_margin_transfer: r.read_bool()?,
        group_delay: r.read_i32()?,
    };

    let dbl_click_panic_sell = r.read_bool()?;
    let coins_black_list_text = r.read_string_x()?;
    let use_coins_black_list = r.read_bool()?;
    let h_pos_black_list_text = r.read_string_x()?;
    let trailing_float = r.read_f64()?;
    let h_pos_black_list_add = r.read_string_x()?;
    let random_price = r.read_bool()?;
    let report_config = parse_report_config(r)?;

    let dont_buy_delisted = r.read_bool()?;
    let dont_buy_new_coins = r.read_i32()?;
    let auto_cancel_lower_buy = r.read_i32()?;
    let buy_on_enter = r.read_bool()?;
    let auto_buy_bnb = r.read_bool()?;
    let auto_buy_bnb_level = r.read_f64()?;
    let auto_buy_bnb_volume = r.read_f64()?;
    let order_replace_click_buy = r.read_u8()?;
    let dont_buy_forward = r.read_bool()?;
    let chart_split_zones = r.read_bool()?;
    let draw_stop = r.read_bool()?;
    let max_order = r.read_f64()?;
    let unlimited_orders = r.read_bool()?;
    let use_manual_strategy = r.read_bool()?;
    let manual_strategy = r.read_string_x()?;
    let x9_mode = r.read_bool()?;
    let x_t_mode = r.read_bool()?;
    let fixed_balance_warning = r.read_bool()?;
    let trades_gap_time = r.read_i32()?;
    let buy_iceberg = r.read_bool()?;
    let sell_iceberg = r.read_bool()?;
    let iceberg_step = r.read_f64()?;
    let order_set_click = r.read_u8()?;
    let pending_order_set_click = r.read_u8()?;
    let pending_orders_spread = r.read_f64()?;
    let pending_orders_spread_h_delta = r.read_f64()?;
    let buy_pending_spread = r.read_i32()?;
    let buy_stop_pending_spread = r.read_i32()?;
    let tm_panic_sell_closed = r.read_bool()?;
    let tm_send_only_share = r.read_bool()?;
    let tm_cancel_closed = r.read_bool()?;
    let tm_cancel_buy_on_fill = r.read_bool()?;
    let multi_commands = r.read_bool()?;
    let iceberg_click_count = r.read_i32()?;
    let tm_send_first_sell = r.read_bool()?;
    let tm_sell_if_master_not_filled = r.read_bool()?;
    let tm_max_order = r.read_f64()?;
    let multi_orders = parse_multi_orders(r)?;
    let order_replace_click_sell = r.read_u8()?;

    // AlertConfig
    let alert_config = AlertConfig {
        enabled: r.read_bool()?,
        keep_time: r.read_i32()?,
        sound_kind: r.read_i32()?,
        repeat_count: r.read_i32()?,
    };

    let use_moon_bl = r.read_bool()?;
    let correct_order_price = r.read_bool()?;
    let f_binance_commission = r.read_f64()?;

    // SpotConfig
    let spot_config = SpotConfig {
        show_trades: r.read_bool()?,
        show_book: r.read_bool()?,
        book_len: r.read_f32()?,
        show_market_avg: r.read_bool()?,
        show_min_max: r.read_bool()?,
        show_avg_price: r.read_bool()?,
        spot_btc: r.read_bool()?,
        shift_spot: r.read_bool()?,
        show_mark_price: r.read_bool()?,
        show_liq: r.read_bool()?,
        huge_liq: r.read_bool()?,
        show_open_int: r.read_bool()?,
        show_avg_price_line: r.read_bool()?,
    };

    let use_book_ticker = r.read_bool()?;
    let exclude_black_list_delta = r.read_bool()?;
    let free_position_check = r.read_bool()?;

    // OrdersControl
    let orders_control = OrdersControl {
        active: r.read_bool()?,
        min_price: r.read_f64()?,
        max_time: r.read_i32()?,
        h_pos_control: r.read_bool()?,
        h_pos_report: r.read_bool()?,
        h_pos_auto_sell: r.read_bool()?,
        ignore_replacing_bug: r.read_bool()?,
        ignore_protection: r.read_i32()?,
        sign_orders: r.read_bool()?,
        liq_control: r.read_bool()?,
    };

    let m_avg_use_vol_weight = r.read_bool()?;
    let pending_buy_price = r.read_bool()?;
    let fixed_sell_mode = r.read_bool()?;
    let fixed_sell_price = r.read_f64()?;
    let futures_rules = r.read_bool()?;

    let cashback_settings = CashBackSettings {
        sep: r.read_bool()?,
        agg_accounts: r.read_bool()?,
        max_days: r.read_i32()?,
        instant_bnb: r.read_bool()?,
        hide_info: r.read_bool()?,
    };

    let auto_lower_lev = r.read_bool()?;

    let transfer_config = AssetTransferConfig {
        show_total: r.read_bool()?,
        hide_zero: r.read_bool()?,
        show_baks: r.read_bool()?,
    };

    let min_balance = r.read_f64()?;

    let auto_manage_lev = AutoManageLevConfig {
        auto_max_order: r.read_bool()?,
        auto_lev_up: r.read_bool()?,
        fix_lev: r.read_i32()?,
        auto_isolated: r.read_bool()?,
        tlg_report: r.read_bool()?,
        auto_cross: r.read_bool()?,
        auto_fix_lev: r.read_bool()?,
    };

    let auto_lev_control = r.read_string_x()?;
    let use_stop_market = r.read_bool()?;
    let auto_close_zero_pos = r.read_bool()?;

    let send_shots_config = SendShotsConfig {
        may_send: r.read_bool()?,
        profit_abs: r.read_i32()?,
        profit_pers: r.read_i32()?,
        profit_session: r.read_i32()?,
        send_negative: r.read_bool()?,
        send_to_open_chat: r.read_bool()?,
        send_public: r.read_bool()?,
        time_scale: r.read_i32()?,
        price_scale: r.read_i32()?,
    };

    let use_lev_for_take = r.read_bool()?;
    let bybit_commission = r.read_f64()?;
    let auto_reduce_order = r.read_bool()?;
    let gate_commission = r.read_f64()?;

    let mut manual_strats_names: [String; 10] = Default::default();
    for name in &mut manual_strats_names {
        *name = r.read_string_x()?;
    }

    // ManualStratsConfig
    let manual_strats_config = ManualStratsConfig {
        use_buttons: r.read_bool()?,
        hot_keys: r.read_u16_array::<10>()?,
        show_button: r.read_bool_array::<10>()?,
    };

    let clear_triggers_string = r.read_string_x()?;
    let no_trades_markets_text = r.read_string_x()?;
    let use_websocket_api = r.read_bool()?;
    let order_book_levels_ws = r.read_i32()?;
    let arb_view_config = parse_arb_view(r)?;

    // Tail fields (gated by blockEnd)
    let deltas_by_trades = if r.pos < block_end {
        r.read_bool()?
    } else {
        false
    };
    let ignore_strat_sell_price = if r.pos < block_end {
        r.read_bool()?
    } else {
        false
    };
    let use_hl_fast_ioc = if r.pos < block_end {
        r.read_bool()?
    } else {
        false
    };

    let unknown_tail = if r.pos < block_end {
        copy_bytes(&r.data[r.pos..block_end])?
    } else {
        Vec::new()
    };

    Ok(TradingSection {
        auto_delete_logs,
        log_level,
        binance_connection,
        auto_start,
        auto_start_2,
        fav_markets,
        x_ask,
        x_sell,
        x_sell_scalp,
        play_with,
        check_glass,
        max_orders,
        auto_sell_partial,
        price_range,
        sell_auto_move,
        pump_q_level,
        sell_x2_level,
        cancel_buy_on_sell_fill,
        auto_cancel_buy_order,
        vol_drop_level,
        price_drop_level,
        panic_if_vol_drop,
        panic_if_price_drop,
        manual_mode,
        draw_all_buy,
        trailing_stop,
        trailing_drop,
        engine_check_buy,
        use_manual_balance,
        fixed_trade_balance,
        use_current_ask,
        g_take_profit,
        use_g_take_profit,
        show_order_comment,
        chart_clean_up_time,
        moonbot_config,
        dbl_click_panic_sell,
        coins_black_list_text,
        use_coins_black_list,
        h_pos_black_list_text,
        trailing_float,
        h_pos_black_list_add,
        random_price,
        report_config,
        dont_buy_delisted,
        dont_buy_new_coins,
        auto_cancel_lower_buy,
        buy_on_enter,
        auto_buy_bnb,
        auto_buy_bnb_level,
        auto_buy_bnb_volume,
        order_replace_click_buy,
        dont_buy_forward,
        chart_split_zones,
        draw_stop,
        max_order,
        unlimited_orders,
        use_manual_strategy,
        manual_strategy,
        x9_mode,
        x_t_mode,
        fixed_balance_warning,
        trades_gap_time,
        buy_iceberg,
        sell_iceberg,
        iceberg_step,
        order_set_click,
        pending_order_set_click,
        pending_orders_spread,
        pending_orders_spread_h_delta,
        buy_pending_spread,
        buy_stop_pending_spread,
        tm_panic_sell_closed,
        tm_send_only_share,
        tm_cancel_closed,
        tm_cancel_buy_on_fill,
        multi_commands,
        iceberg_click_count,
        tm_send_first_sell,
        tm_sell_if_master_not_filled,
        tm_max_order,
        multi_orders,
        order_replace_click_sell,
        alert_config,
        use_moon_bl,
        correct_order_price,
        f_binance_commission,
        spot_config,
        use_book_ticker,
        exclude_black_list_delta,
        free_position_check,
        orders_control,
        m_avg_use_vol_weight,
        pending_buy_price,
        fixed_sell_mode,
        fixed_sell_price,
        futures_rules,
        cashback_settings,
        auto_lower_lev,
        transfer_config,
        min_balance,
        auto_manage_lev,
        auto_lev_control,
        use_stop_market,
        auto_close_zero_pos,
        send_shots_config,
        use_lev_for_take,
        bybit_commission,
        auto_reduce_order,
        gate_commission,
        manual_strats_names,
        manual_strats_config,
        clear_triggers_string,
        no_trades_markets_text,
        use_websocket_api,
        order_book_levels_ws,
        arb_view_config,
        deltas_by_trades,
        ignore_strat_sell_price,
        use_hl_fast_ioc,
        unknown_tail,
    })
}

fn parse_custom_draw_tool(r: &mut Reader) -> Result<CustomDrawTool, SharedConfigError> {
    Ok(CustomDrawTool {
        color_w: r.read_u32()?,
        color_b: r.read_u32()?,
        fill_color_w: r.read_u32()?,
        fill_color_b: r.read_u32()?,
        sound_alert: r.read_bool()?,
        sound_kind: r.read_i32()?,
        thickness: r.read_f32()?,
        stroke: r.read_i32()?, // TStrokeDash, 4 bytes ({$MINENUMSIZE 4})
        thickness_2: r.read_f32()?,
        stroke_2: r.read_i32()?,
        color_2w: r.read_u32_array::<6>()?,
        color_2b: r.read_u32_array::<6>()?,
        fill_color_2w: r.read_u32_array::<6>()?,
        fill_color_2b: r.read_u32_array::<6>()?,
        emulate_trades: r.read_bool()?,
        prevent_switch: r.read_bool()?,
    })
}

fn parse_visual(r: &mut Reader, block_end: usize) -> Result<VisualSection, SharedConfigError> {
    let ver = r.read_u8()?;
    if ver != 2 {
        return err("wrong shared visual version");
    }

    // IntColors (8 fields)
    let colors = IntColors {
        buy_order_color_bu: r.read_u32()?,
        sell_color_u: r.read_u32()?,
        buy_order_color_wu: r.read_u32()?,
        sell_order_color_u: r.read_u32()?,
        buy_order_done_color_u: r.read_u32()?,
        sell_order_done_color_u: r.read_u32()?,
        trailing_color_u: r.read_u32()?,
        price_line_width: r.read_i32()?,
    };

    let panic_sell_opacity = r.read_i32()?;
    let hide_forum_label = r.read_bool()?;
    let show_red_green_lines = r.read_bool()?;
    let scrolling_charts = r.read_bool()?;
    let startup_load_charts = r.read_bool()?;
    let auto_close_charts = r.read_bool()?;
    let left_chart_info = r.read_bool()?;
    let show_red_green_dots = r.read_bool()?;
    let glass_opacity = r.read_i32()?;
    let chart_time_scale = r.read_i32()?;
    let chart_watch_multi_frames = r.read_bool()?;
    let chart_history_len = r.read_i32()?;
    let hide_right_chart_panel = r.read_bool()?;
    let show_vert_volume = r.read_bool()?;
    let vert_vol_time_frame = r.read_i32()?;
    let vert_vol_opacity = r.read_f32()?;
    let vert_vol_height = r.read_f32()?;
    let vert_vol_kind = r.read_i32()?;
    let v_tool_show = r.read_bool()?;
    let v_tool_time = r.read_f64()?;
    let v_tool_opacity = r.read_f32()?;
    let v_tool_show_buyers = r.read_bool()?;
    let v_tool_time_index = r.read_i32()?;
    let hv_show = r.read_bool()?;
    let hv_width = r.read_f32()?;
    let hv_time_frame_ind = r.read_i32()?;
    let hv_time_frame = r.read_i32()?;
    let hv_price_frame = r.read_f32()?;
    let hv_opacity = r.read_f32()?;

    // CustomDrawConfig
    let cdc_ver = r.read_i32()?;
    let cdc_opacity = r.read_f32()?;
    let cdc_show = r.read_bool()?;
    let mut cdc_tools: [CustomDrawTool; 16] = std::array::from_fn(|_| CustomDrawTool::default());
    for tool in &mut cdc_tools {
        *tool = parse_custom_draw_tool(r)?;
    }
    let custom_draw_config = CustomDrawConfig {
        ver: cdc_ver,
        f_opacity: cdc_opacity,
        f_show: cdc_show,
        tools: cdc_tools,
    };

    let global_current_draw_kind = r.read_u8()?;
    let hv_disp_vol = r.read_i32()?;
    let hv_kind = r.read_i32()?;
    let show_market_captions = r.read_bool()?;
    let show_orders_captions = r.read_bool()?;
    let orders_captions_lower = r.read_bool()?;
    let show_usd_on_charts = r.read_bool()?;
    let new_markets_max_scale = r.read_bool()?;
    let vert_vol_ind_pos = r.read_i32()?;
    let book_cumulative_opacity = r.read_i32()?;
    let book_orders_opacity = r.read_i32()?;
    let book_orders_width = r.read_i32()?;

    // BlinkConfig
    let blink_config = BlinkConfig {
        blink_btc: r.read_bool()?,
        blink_btc_delta: r.read_f64()?,
        blink_btc_delta_up: r.read_f64()?,
        alarm_btc: r.read_bool()?,
        alarm_type: r.read_u8()?,
    };

    let charts_auto_shift = r.read_bool()?;
    let show_iceberg = r.read_bool()?;
    let hide_pnl = r.read_bool()?;
    let draw_cur_price = r.read_i32()?;
    let auto_request_charts = r.read_bool()?;
    let vo_sort_kind = r.read_u8()?;
    let vo_older_first = r.read_bool()?;
    let vo_sort_by_market = r.read_bool()?;
    let hide_buy_button = r.read_bool()?;
    let show_strat_numbers = r.read_bool()?;

    // ShowFilters (8 fields)
    let show_filters = ShowFilters {
        need_show: r.read_bool()?,
        filters: r.read_bool()?,
        sessions: r.read_bool()?,
        hide_zero: r.read_bool()?,
        m_shot_area: r.read_bool()?,
        scale_tool: r.read_bool()?,
        scroll_filters: r.read_bool()?,
        show_detects: r.read_bool()?,
    };

    let hide_cashback_button = r.read_bool()?;
    let hide_ny_elka = r.read_bool()?;

    // ChartsSettings (versioned sub-block)
    let cs_ver = r.read_u8()?;
    if cs_ver != 1 {
        return err("wrong shared charts settings version");
    }
    let charts_settings = ChartsSettings {
        auto_arrange: r.read_bool()?,
        visible: r.read_bool()?,
        wide: r.read_bool()?,
        stay_on_top: r.read_bool()?,
        max_charts: r.read_i32()?,
        refresh: r.read_i32()?,
        x1: r.read_f64()?,
        x2: r.read_f64()?,
    };

    let spy_hide_mode = r.read_bool()?;

    // HeatMapConfig
    let heatmap_config = HeatMapConfig {
        show: r.read_bool()?,
        use_q: r.read_bool()?,
        height: r.read_f32()?,
        cpu: r.read_bool()?,
        trades: r.read_bool()?,
        app_latency: r.read_bool()?,
        draw_latency: r.read_bool()?,
    };

    let icon_selection = r.read_i32()?;
    let remember_chart_buttons = r.read_bool()?;
    let show_detects_tool = r.read_bool()?;
    let scale_plus_index = r.read_i32()?;
    let scale_minus_index = r.read_i32()?;

    // NewsFormConfig
    let news_form_config = NewsFormConfig {
        stay_on_top: r.read_bool()?,
        exact_time: r.read_bool()?,
        font_size: r.read_u8()?,
        strength: r.read_u8()?,
        update_orig: r.read_bool()?,
        sound: r.read_u8()?,
        theme: r.read_i32()?,
        coin_card_font_size: r.read_i32()?,
        full_tags: r.read_bool()?,
        feed_mode: r.read_bool()?,
    };

    // FontSizes
    let font_sizes = r.read_u8_array::<20>()?;

    // Tail fields (gated by blockEnd)
    let chart_candles_style = if r.pos < block_end { r.read_u8()? } else { 2 };
    let chart_candles_tick_opacity = if r.pos < block_end { r.read_u8()? } else { 25 };
    let chart_candles_neutral_ticks = if r.pos < block_end {
        r.read_bool()?
    } else {
        false
    };
    let chart_candles_outline_width = if r.pos < block_end { r.read_u8()? } else { 2 };
    let chart_candles_tick_wicks = if r.pos < block_end {
        r.read_bool()?
    } else {
        false
    };

    let mut candle_colors = [CandleColorSet::default(), CandleColorSet::default()];
    for cc in &mut candle_colors {
        cc.green = if r.pos < block_end { r.read_u32()? } else { 0 };
        cc.red = if r.pos < block_end { r.read_u32()? } else { 0 };
        cc.neutral = if r.pos < block_end { r.read_u32()? } else { 0 };
    }

    let use_ai_coin_card = if r.pos < block_end {
        r.read_bool()?
    } else {
        false
    };
    let ai_card_provider = if r.pos < block_end { r.read_u8()? } else { 0 };
    let ai_card_model = if r.pos < block_end {
        r.read_string_x()?
    } else {
        String::new()
    };
    let ai_card_prompt = if r.pos < block_end {
        r.read_string_x()?
    } else {
        String::new()
    };
    let manual_charts_full_screen = if r.pos < block_end {
        r.read_bool()?
    } else {
        false
    };

    let unknown_tail = if r.pos < block_end {
        copy_bytes(&r.data[r.pos..block_end])?
    } else {
        Vec::new()
    };

    Ok(VisualSection {
        colors,
        panic_sell_opacity,
        hide_forum_label,
        show_red_green_lines,
        scrolling_charts,
        startup_load_charts,
        auto_close_charts,
        left_chart_info,
        show_red_green_dots,
        glass_opacity,
        chart_time_scale,
        chart_watch_multi_frames,
        chart_history_len,
        hide_right_chart_panel,
        show_vert_volume,
        vert_vol_time_frame,
        vert_vol_opacity,
        vert_vol_height,
        vert_vol_kind,
        v_tool_show,
        v_tool_time,
        v_tool_opacity,
        v_tool_show_buyers,
        v_tool_time_index,
        hv_show,
        hv_width,
        hv_time_frame_ind,
        hv_time_frame,
        hv_price_frame,
        hv_opacity,
        custom_draw_config,
        global_current_draw_kind,
        hv_disp_vol,
        hv_kind,
        show_market_captions,
        show_orders_captions,
        orders_captions_lower,
        show_usd_on_charts,
        new_markets_max_scale,
        vert_vol_ind_pos,
        book_cumulative_opacity,
        book_orders_opacity,
        book_orders_width,
        blink_config,
        charts_auto_shift,
        show_iceberg,
        hide_pnl,
        draw_cur_price,
        auto_request_charts,
        vo_sort_kind,
        vo_older_first,
        vo_sort_by_market,
        hide_buy_button,
        show_strat_numbers,
        show_filters,
        hide_cashback_button,
        hide_ny_elka,
        charts_settings,
        spy_hide_mode,
        heatmap_config,
        icon_selection,
        remember_chart_buttons,
        show_detects_tool,
        scale_plus_index,
        scale_minus_index,
        news_form_config,
        font_sizes,
        chart_candles_style,
        chart_candles_tick_opacity,
        chart_candles_neutral_ticks,
        chart_candles_outline_width,
        chart_candles_tick_wicks,
        candle_colors,
        use_ai_coin_card,
        ai_card_provider,
        ai_card_model,
        ai_card_prompt,
        manual_charts_full_screen,
        unknown_tail,
    })
}

fn parse_ini_sections(
    r: &mut Reader,
    allowed: &[&str],
) -> Result<Vec<IniSectionData>, SharedConfigError> {
    let count = r.read_i32()?;
    if count < 0 || count > allowed.len() as i32 {
        return err("wrong shared config ini section count");
    }
    let mut result = Vec::new();
    result
        .try_reserve(count as usize)
        .map_err(|err| SharedConfigError::new(format!("shared config allocation failed: {err}")))?;
    for _ in 0..count {
        let name = r.read_string_x()?;
        if !allowed.iter().any(|&a| a.eq_ignore_ascii_case(&name)) {
            return err("wrong shared config ini section");
        }
        let entry_count = r.read_i32()?;
        if !(0..=MAX_INI_ENTRIES).contains(&entry_count) {
            return err("wrong shared config ini entry count");
        }
        let mut entries = Vec::new();
        entries.try_reserve(entry_count as usize).map_err(|err| {
            SharedConfigError::new(format!("shared config allocation failed: {err}"))
        })?;
        for _ in 0..entry_count {
            let key = r.read_string_x()?;
            let value = r.read_string_x()?;
            entries.push((key, value));
        }
        result.push(IniSectionData { name, entries });
    }
    Ok(result)
}

fn parse_theme(r: &mut Reader, block_end: usize) -> Result<ThemeSection, SharedConfigError> {
    let ver = r.read_u8()?;
    if ver != 1 {
        return err("wrong shared theme version");
    }
    let current_style = r.read_i32()?;
    let ini_sections = parse_ini_sections(r, &["ColorsLight", "ColorsDark"])?;
    let unknown_tail = if r.pos < block_end {
        copy_bytes(&r.data[r.pos..block_end])?
    } else {
        Vec::new()
    };
    Ok(ThemeSection {
        current_style,
        ini_sections,
        unknown_tail,
    })
}

fn parse_ini(r: &mut Reader, block_end: usize) -> Result<IniSection, SharedConfigError> {
    let ver = r.read_u8()?;
    if ver != 1 {
        return err("wrong shared ini version");
    }
    let ini_sections = parse_ini_sections(r, &["Charts", "ArbColors"])?;
    let unknown_tail = if r.pos < block_end {
        copy_bytes(&r.data[r.pos..block_end])?
    } else {
        Vec::new()
    };
    Ok(IniSection {
        ini_sections,
        unknown_tail,
    })
}

fn parse_hotkeys(r: &mut Reader) -> Result<HotkeysConfig, SharedConfigError> {
    Ok(HotkeysConfig {
        filled: r.read_bool()?,
        ver: r.read_u8()?,
        o_size: r.read_f64_array::<6>()?,
        b_num: r.read_i32()?,
        o_keys: r.read_u16_array::<6>()?,
        split_parts: r.read_u8()?,
        sb_num: r.read_u8()?,
        s_keys: r.read_u16_array::<6>()?,
        s_price: r.read_f32_array::<6>()?,
        cancel_buy: r.read_u16()?,
        panic_sell: r.read_u16()?,
        join_sells: r.read_u16()?,
        switch_charts: r.read_u16()?,
        reload_book: r.read_u16()?,
        new_long: r.read_u16()?,
        new_short: r.read_u16()?,
        split_order: r.read_u16()?,
        shift_buy_up: r.read_u16()?,
        shift_buy_down: r.read_u16()?,
        shift_sell_up: r.read_u16()?,
        shift_sell_down: r.read_u16()?,
        make_shot: r.read_u16()?,
        make_shot_bot: r.read_u16()?,
        reload_chart: r.read_u16()?,
        scale_plus: r.read_u16()?,
        scale_minus: r.read_u16()?,
        sell_plus: r.read_u16()?,
        sell_minus: r.read_u16()?,
        spy_mode: r.read_u16()?,
        show_charts: r.read_u16()?,
        split_order_x: r.read_u16()?,
        switch_figure: r.read_u16()?,
        fit_sells: r.read_u16()?,
        panic_sell_one: r.read_u16()?,
        cancel_all_buys: r.read_u16()?,
        broadcast: r.read_u16()?,
    })
}

fn parse_ui(r: &mut Reader, block_end: usize) -> Result<UiSection, SharedConfigError> {
    let ver = r.read_u8()?;
    if ver != 3 {
        return err("wrong shared ui version");
    }

    let hide_demo_button = r.read_bool()?;
    let confirm_close = r.read_bool()?;
    let new_markets_on_top = r.read_bool()?;
    let coins_sort_order = r.read_i32()?;
    let hotkeys_config = parse_hotkeys(r)?;
    let strat_editor_chapters = r.read_string_x()?;

    // MarketsTableConfig
    let markets_table_config = MarketsTableConfig {
        sort_col: r.read_i32()?,
        col_vis: r.read_bool_array::<41>()?,
        col_pos: r.read_u8_array::<41>()?,
    };

    let main_button_index_1 = r.read_u8()?;
    let strat_expanded_state = r.read_bool_array::<11>()?;

    let unknown_tail = if r.pos < block_end {
        copy_bytes(&r.data[r.pos..block_end])?
    } else {
        Vec::new()
    };

    Ok(UiSection {
        hide_demo_button,
        confirm_close,
        new_markets_on_top,
        coins_sort_order,
        hotkeys_config,
        strat_editor_chapters,
        markets_table_config,
        main_button_index_1,
        strat_expanded_state,
        unknown_tail,
    })
}

// ---------------------------------------------------------------------------
// Section serializers
// ---------------------------------------------------------------------------

fn write_signal_config(w: &mut Writer, c: &SignalConfig) {
    w.write_i32(c.ver);
    w.write_bool(c.use_keywords);
    w.write_bool(c.use_token_tags);
    w.write_i32(c.buy_key_dist);
    w.write_bool(c.only_1_token);
    w.write_bool(c.buy_if_price_found);
    w.write_i32(c.x_found_price);
    w.write_bool(c.use_black_words);
    w.write_i32(c.words_count);
    w.write_bool(c.use_words_count);
    w.write_bool(c.use_price);
    w.write_bool(c.use_stops);
    w.write_bool(c.tokens_no_tags);
    w.write_bool(c.token_links);
    w.write_bool(c.use_lower_price_words);
    w.write_i32(c.x_lower_price);
    w.write_bool(c.special_formats);
}

fn write_signals(w: &mut Writer, s: &SignalsSection) {
    w.write_u8(2); // version
    w.write_string_x(&s.pump_channel);
    w.write_string_list_x(&s.pump_channels);
    w.write_bool(s.do_monitoring);
    w.write_bool(s.look_full_link_tlg);
    w.write_bool(s.multi_channels);
    w.write_bool(s.telegram_auto_buy);
    w.write_bool(s.load_deep_history);
    w.write_bool(s.only_secret_signal);
    w.write_bool(s.more_then_1_channel);
    w.write_bool(s.clipboard_auto_buy);
    w.write_bool(s.autodetect_auto_buy);
    w.write_i32(s.signal_sound);
    w.write_bool(s.play_signal_sound);
    w.write_bool(s.lower_case_token_tlg);
    w.write_bool(s.show_cross);
    w.write_bool(s.monitor_clipboard);
    w.write_bool(s.lower_case_token_cbd);
    w.write_bool(s.look_full_link_cbd);
    w.write_string_x(&s.msg_keywords_long);
    w.write_string_x(&s.msg_keywords_short);
    w.write_string_x(&s.msg_token_tags);
    w.write_string_x(&s.msg_black_words);
    write_signal_config(w, &s.signal_config);
    w.write_bool(s.advanced_filter);
    w.write_bool(s.auto_show_on_signal);
    w.write_bool(s.advanced_filter_clipboard);
    w.write_i32(s.base_vol_ind_type);
    w.write_i32(s.max_too_long_msg_len);
    w.write_i32(s.signal_sound_2);
    w.write_i32(s.sell_alert_level);
    w.write_bool(s.play_sell_alert);
    w.write_i32(s.last_used_order_type);
    w.write_bool(s.full_screen_prevent_signals);
    w.write_bool(s.listen_moon_channel);
    w.write_string_x(&s.lower_price_words);
    w.write_bool(s.dont_buy_reply);
    w.write_i32(s.base_vol_n);
    w.write_bool(s.use_last_detect_caption);
    w.write_i32(s.buy_signal_sound);
    w.write_bool(s.play_buy_alert);
    w.write_i32(s.buy_alert_level);
    w.write_string_x(&s.strats_filter);
    w.write_string_x(&s.news_tags_filter);
    w.write_string_x(&s.news_tokens_filter);
    w.write_bytes(&s.unknown_tail);
}

fn write_auto_start(w: &mut Writer, a: &AutoStartConfig) {
    w.write_bool(a.auto_start);
    w.write_bool(a.auto_detect_on);
    w.write_bool(a.strategies_on);
    w.write_bool(a.work_time);
    w.write_bool(a.auto_stop_if_loss);
    w.write_bool(a.remember_state);
    w.write_bool(a.sell_if_loss);
    w.write_bool(a.dont_wait_sells);
    w.write_f64(a.auto_stop_loss);
    w.write_bool(a.panic_btc);
    w.write_bool(a.panic_market);
    w.write_bool(a.auto_stop_if_loss_hours);
    w.write_bool(a.auto_update);
    w.write_bool(a.restart_after_err);
    w.write_bool(a.restart_after_ping);
    w.write_bool(a.ignore_emulator);
    w.write_i32(a.stop_trades);
    w.write_i32(a.restart_err_time);
    w.write_f64(a.panic_btc_delta);
    w.write_f64(a.panic_market_delta);
    w.write_bool(a.auto_stop_on_errors);
    w.write_bool(a.auto_stop_on_ping);
    w.write_bool(a.sell_all_on_errors);
    w.write_bool(a.sell_all_on_ping);
    w.write_i32(a.errors_level);
    w.write_i32(a.ping_level);
    w.write_i32(a.restart_ping_time);
    w.write_f64(a.auto_stop_hours_val);
    w.write_i32(a.stop_hours);
    w.write_i32(a.stop_hours_trades);
    w.write_f64(a.panic_btc_delta_up);
    w.write_f64(a.work_time_from);
    w.write_f64(a.work_time_to);
}

fn write_auto_start_2(w: &mut Writer, a: &AutoStartConfig2) {
    w.write_bool(a.restart_on_market);
    w.write_f64(a.btc_higher_then);
    w.write_f64(a.btc_lower_then);
    w.write_f64(a.market_higher_then);
    w.write_bool(a.show_old_listing);
    w.write_bool(a.reset_session);
    w.write_i32(a.max_session_cap);
    w.write_i32(a.rs_hours);
}

fn write_multi_orders(w: &mut Writer, m: &MultiOrdersConfig) {
    w.write_u8(m.ver);
    w.write_bool(m.use_multi_orders);
    w.write_u8(m.buy_set_click);
    w.write_u8(m.buy_move_click);
    w.write_u8(m.sell_move_click);
    w.write_u8(m.replace_buy_kind);
    w.write_u8(m.replace_sell_kind);
    w.write_i32(m.split_sells);
    w.write_bool(m.show_orders_num);
    w.write_bool(m.kir_style);
    w.write_u8(m.fix_pos);
    w.write_u8(m.join_sell_kind);
    w.write_u8(m.short_set_click);
    w.write_u8(m.pending_short_set_click);
    w.write_f32(m.done_opacity);
    w.write_u8(m.buy_move_click_2);
    w.write_u8(m.sell_move_click_2);
    w.write_u8(m.replace_buy_kind_2);
    w.write_u8(m.replace_sell_kind_2);
    w.write_bool(m.same_hotkeys_for_move);
    w.write_u8(m.short_buy_move_click);
    w.write_u8(m.short_sell_move_click);
    w.write_u8(m.short_buy_move_click_2);
    w.write_u8(m.short_sell_move_click_2);
}

fn write_report_config(w: &mut Writer, c: &ReportConfig) {
    w.write_bool_array(&c.fields_vis);
    w.write_u16_array(&c.fields_width);
    w.write_bool(c.only_active);
    w.write_i32(c.range);
    w.write_u8_array(&c.filter1);
    w.write_u8_array(&c.filter2);
    w.write_u8_array(&c.filter3);
    w.write_u8(c.active_type);
    w.write_bool(c.rep_emulator_orders);
    w.write_bool(c.stretch);
    w.write_bool(c.only_2_orders);
    w.write_bool(c.use_leverage);
    w.write_u16(c.max_lines);
    w.write_u8(c.pos_direction);
    w.write_bool(c.store_liq);
    w.write_bool(c.store_fund);
    w.write_bool(c.by_close_date);
}

fn write_trading(w: &mut Writer, t: &TradingSection) {
    w.write_u8(3); // version
    w.write_i32(t.auto_delete_logs);
    w.write_i32(t.log_level);
    w.write_i32(t.binance_connection);
    write_auto_start(w, &t.auto_start);
    write_auto_start_2(w, &t.auto_start_2);
    w.write_string_x(&t.fav_markets);
    w.write_i32(t.x_ask);
    w.write_i32(t.x_sell);
    w.write_i32(t.x_sell_scalp);
    w.write_i32(t.play_with);
    w.write_bool(t.check_glass);
    w.write_i32(t.max_orders);
    w.write_i32(t.auto_sell_partial);
    w.write_i32(t.price_range);
    w.write_bool(t.sell_auto_move);
    w.write_i32(t.pump_q_level);
    w.write_i32(t.sell_x2_level);
    w.write_bool(t.cancel_buy_on_sell_fill);
    w.write_i32(t.auto_cancel_buy_order);
    w.write_i32(t.vol_drop_level);
    w.write_f32(t.price_drop_level);
    w.write_bool(t.panic_if_vol_drop);
    w.write_bool(t.panic_if_price_drop);
    w.write_bool(t.manual_mode);
    w.write_bool(t.draw_all_buy);
    w.write_bool(t.trailing_stop);
    w.write_f32(t.trailing_drop);
    w.write_bool(t.engine_check_buy);
    w.write_bool(t.use_manual_balance);
    w.write_f64(t.fixed_trade_balance);
    w.write_bool(t.use_current_ask);
    w.write_f64(t.g_take_profit);
    w.write_bool(t.use_g_take_profit);
    w.write_bool(t.show_order_comment);
    w.write_i32(t.chart_clean_up_time);
    // MoonBotConfig
    w.write_bool(t.moonbot_config.silent);
    w.write_bool(t.moonbot_config.auto_margin_borrow);
    w.write_bool(t.moonbot_config.auto_margin_transfer);
    w.write_i32(t.moonbot_config.group_delay);
    w.write_bool(t.dbl_click_panic_sell);
    w.write_string_x(&t.coins_black_list_text);
    w.write_bool(t.use_coins_black_list);
    w.write_string_x(&t.h_pos_black_list_text);
    w.write_f64(t.trailing_float);
    w.write_string_x(&t.h_pos_black_list_add);
    w.write_bool(t.random_price);
    write_report_config(w, &t.report_config);
    w.write_bool(t.dont_buy_delisted);
    w.write_i32(t.dont_buy_new_coins);
    w.write_i32(t.auto_cancel_lower_buy);
    w.write_bool(t.buy_on_enter);
    w.write_bool(t.auto_buy_bnb);
    w.write_f64(t.auto_buy_bnb_level);
    w.write_f64(t.auto_buy_bnb_volume);
    w.write_u8(t.order_replace_click_buy);
    w.write_bool(t.dont_buy_forward);
    w.write_bool(t.chart_split_zones);
    w.write_bool(t.draw_stop);
    w.write_f64(t.max_order);
    w.write_bool(t.unlimited_orders);
    w.write_bool(t.use_manual_strategy);
    w.write_string_x(&t.manual_strategy);
    w.write_bool(t.x9_mode);
    w.write_bool(t.x_t_mode);
    w.write_bool(t.fixed_balance_warning);
    w.write_i32(t.trades_gap_time);
    w.write_bool(t.buy_iceberg);
    w.write_bool(t.sell_iceberg);
    w.write_f64(t.iceberg_step);
    w.write_u8(t.order_set_click);
    w.write_u8(t.pending_order_set_click);
    w.write_f64(t.pending_orders_spread);
    w.write_f64(t.pending_orders_spread_h_delta);
    w.write_i32(t.buy_pending_spread);
    w.write_i32(t.buy_stop_pending_spread);
    w.write_bool(t.tm_panic_sell_closed);
    w.write_bool(t.tm_send_only_share);
    w.write_bool(t.tm_cancel_closed);
    w.write_bool(t.tm_cancel_buy_on_fill);
    w.write_bool(t.multi_commands);
    w.write_i32(t.iceberg_click_count);
    w.write_bool(t.tm_send_first_sell);
    w.write_bool(t.tm_sell_if_master_not_filled);
    w.write_f64(t.tm_max_order);
    write_multi_orders(w, &t.multi_orders);
    w.write_u8(t.order_replace_click_sell);
    // AlertConfig
    w.write_bool(t.alert_config.enabled);
    w.write_i32(t.alert_config.keep_time);
    w.write_i32(t.alert_config.sound_kind);
    w.write_i32(t.alert_config.repeat_count);
    w.write_bool(t.use_moon_bl);
    w.write_bool(t.correct_order_price);
    w.write_f64(t.f_binance_commission);
    // SpotConfig
    let sp = &t.spot_config;
    w.write_bool(sp.show_trades);
    w.write_bool(sp.show_book);
    w.write_f32(sp.book_len);
    w.write_bool(sp.show_market_avg);
    w.write_bool(sp.show_min_max);
    w.write_bool(sp.show_avg_price);
    w.write_bool(sp.spot_btc);
    w.write_bool(sp.shift_spot);
    w.write_bool(sp.show_mark_price);
    w.write_bool(sp.show_liq);
    w.write_bool(sp.huge_liq);
    w.write_bool(sp.show_open_int);
    w.write_bool(sp.show_avg_price_line);
    w.write_bool(t.use_book_ticker);
    w.write_bool(t.exclude_black_list_delta);
    w.write_bool(t.free_position_check);
    // OrdersControl
    let oc = &t.orders_control;
    w.write_bool(oc.active);
    w.write_f64(oc.min_price);
    w.write_i32(oc.max_time);
    w.write_bool(oc.h_pos_control);
    w.write_bool(oc.h_pos_report);
    w.write_bool(oc.h_pos_auto_sell);
    w.write_bool(oc.ignore_replacing_bug);
    w.write_i32(oc.ignore_protection);
    w.write_bool(oc.sign_orders);
    w.write_bool(oc.liq_control);
    w.write_bool(t.m_avg_use_vol_weight);
    w.write_bool(t.pending_buy_price);
    w.write_bool(t.fixed_sell_mode);
    w.write_f64(t.fixed_sell_price);
    w.write_bool(t.futures_rules);
    // CashBackSettings
    let cb = &t.cashback_settings;
    w.write_bool(cb.sep);
    w.write_bool(cb.agg_accounts);
    w.write_i32(cb.max_days);
    w.write_bool(cb.instant_bnb);
    w.write_bool(cb.hide_info);
    w.write_bool(t.auto_lower_lev);
    // TransferConfig
    w.write_bool(t.transfer_config.show_total);
    w.write_bool(t.transfer_config.hide_zero);
    w.write_bool(t.transfer_config.show_baks);
    w.write_f64(t.min_balance);
    // AutoManageLev
    let al = &t.auto_manage_lev;
    w.write_bool(al.auto_max_order);
    w.write_bool(al.auto_lev_up);
    w.write_i32(al.fix_lev);
    w.write_bool(al.auto_isolated);
    w.write_bool(al.tlg_report);
    w.write_bool(al.auto_cross);
    w.write_bool(al.auto_fix_lev);
    w.write_string_x(&t.auto_lev_control);
    w.write_bool(t.use_stop_market);
    w.write_bool(t.auto_close_zero_pos);
    // SendShotsConfig
    let ss = &t.send_shots_config;
    w.write_bool(ss.may_send);
    w.write_i32(ss.profit_abs);
    w.write_i32(ss.profit_pers);
    w.write_i32(ss.profit_session);
    w.write_bool(ss.send_negative);
    w.write_bool(ss.send_to_open_chat);
    w.write_bool(ss.send_public);
    w.write_i32(ss.time_scale);
    w.write_i32(ss.price_scale);
    w.write_bool(t.use_lev_for_take);
    w.write_f64(t.bybit_commission);
    w.write_bool(t.auto_reduce_order);
    w.write_f64(t.gate_commission);
    for name in &t.manual_strats_names {
        w.write_string_x(name);
    }
    // ManualStratsConfig
    w.write_bool(t.manual_strats_config.use_buttons);
    w.write_u16_array(&t.manual_strats_config.hot_keys);
    w.write_bool_array(&t.manual_strats_config.show_button);
    w.write_string_x(&t.clear_triggers_string);
    w.write_string_x(&t.no_trades_markets_text);
    w.write_bool(t.use_websocket_api);
    w.write_i32(t.order_book_levels_ws);
    // ArbViewConfig
    w.write_u8(t.arb_view_config.ver);
    w.write_bool(t.arb_view_config.show_absolute);
    w.write_bool(t.arb_view_config.show_numbers);
    w.write_bool(t.arb_view_config.show_lines);
    w.write_bool(t.arb_view_config.show_percent);
    w.write_bool(t.arb_view_config.show_right);
    // Tail fields
    w.write_bool(t.deltas_by_trades);
    w.write_bool(t.ignore_strat_sell_price);
    w.write_bool(t.use_hl_fast_ioc);
    w.write_bytes(&t.unknown_tail);
}

fn write_custom_draw_tool(w: &mut Writer, t: &CustomDrawTool) {
    w.write_u32(t.color_w);
    w.write_u32(t.color_b);
    w.write_u32(t.fill_color_w);
    w.write_u32(t.fill_color_b);
    w.write_bool(t.sound_alert);
    w.write_i32(t.sound_kind);
    w.write_f32(t.thickness);
    w.write_i32(t.stroke);
    w.write_f32(t.thickness_2);
    w.write_i32(t.stroke_2);
    w.write_u32_array(&t.color_2w);
    w.write_u32_array(&t.color_2b);
    w.write_u32_array(&t.fill_color_2w);
    w.write_u32_array(&t.fill_color_2b);
    w.write_bool(t.emulate_trades);
    w.write_bool(t.prevent_switch);
}

fn write_visual(w: &mut Writer, v: &VisualSection) {
    w.write_u8(2); // version
                   // IntColors
    let c = &v.colors;
    w.write_u32(c.buy_order_color_bu);
    w.write_u32(c.sell_color_u);
    w.write_u32(c.buy_order_color_wu);
    w.write_u32(c.sell_order_color_u);
    w.write_u32(c.buy_order_done_color_u);
    w.write_u32(c.sell_order_done_color_u);
    w.write_u32(c.trailing_color_u);
    w.write_i32(c.price_line_width);
    w.write_i32(v.panic_sell_opacity);
    w.write_bool(v.hide_forum_label);
    w.write_bool(v.show_red_green_lines);
    w.write_bool(v.scrolling_charts);
    w.write_bool(v.startup_load_charts);
    w.write_bool(v.auto_close_charts);
    w.write_bool(v.left_chart_info);
    w.write_bool(v.show_red_green_dots);
    w.write_i32(v.glass_opacity);
    w.write_i32(v.chart_time_scale);
    w.write_bool(v.chart_watch_multi_frames);
    w.write_i32(v.chart_history_len);
    w.write_bool(v.hide_right_chart_panel);
    w.write_bool(v.show_vert_volume);
    w.write_i32(v.vert_vol_time_frame);
    w.write_f32(v.vert_vol_opacity);
    w.write_f32(v.vert_vol_height);
    w.write_i32(v.vert_vol_kind);
    w.write_bool(v.v_tool_show);
    w.write_f64(v.v_tool_time);
    w.write_f32(v.v_tool_opacity);
    w.write_bool(v.v_tool_show_buyers);
    w.write_i32(v.v_tool_time_index);
    w.write_bool(v.hv_show);
    w.write_f32(v.hv_width);
    w.write_i32(v.hv_time_frame_ind);
    w.write_i32(v.hv_time_frame);
    w.write_f32(v.hv_price_frame);
    w.write_f32(v.hv_opacity);
    // CustomDrawConfig
    w.write_i32(v.custom_draw_config.ver);
    w.write_f32(v.custom_draw_config.f_opacity);
    w.write_bool(v.custom_draw_config.f_show);
    for tool in &v.custom_draw_config.tools {
        write_custom_draw_tool(w, tool);
    }
    w.write_u8(v.global_current_draw_kind);
    w.write_i32(v.hv_disp_vol);
    w.write_i32(v.hv_kind);
    w.write_bool(v.show_market_captions);
    w.write_bool(v.show_orders_captions);
    w.write_bool(v.orders_captions_lower);
    w.write_bool(v.show_usd_on_charts);
    w.write_bool(v.new_markets_max_scale);
    w.write_i32(v.vert_vol_ind_pos);
    w.write_i32(v.book_cumulative_opacity);
    w.write_i32(v.book_orders_opacity);
    w.write_i32(v.book_orders_width);
    // BlinkConfig
    w.write_bool(v.blink_config.blink_btc);
    w.write_f64(v.blink_config.blink_btc_delta);
    w.write_f64(v.blink_config.blink_btc_delta_up);
    w.write_bool(v.blink_config.alarm_btc);
    w.write_u8(v.blink_config.alarm_type);
    w.write_bool(v.charts_auto_shift);
    w.write_bool(v.show_iceberg);
    w.write_bool(v.hide_pnl);
    w.write_i32(v.draw_cur_price);
    w.write_bool(v.auto_request_charts);
    w.write_u8(v.vo_sort_kind);
    w.write_bool(v.vo_older_first);
    w.write_bool(v.vo_sort_by_market);
    w.write_bool(v.hide_buy_button);
    w.write_bool(v.show_strat_numbers);
    // ShowFilters
    let sf = &v.show_filters;
    w.write_bool(sf.need_show);
    w.write_bool(sf.filters);
    w.write_bool(sf.sessions);
    w.write_bool(sf.hide_zero);
    w.write_bool(sf.m_shot_area);
    w.write_bool(sf.scale_tool);
    w.write_bool(sf.scroll_filters);
    w.write_bool(sf.show_detects);
    w.write_bool(v.hide_cashback_button);
    w.write_bool(v.hide_ny_elka);
    // ChartsSettings (versioned)
    w.write_u8(1);
    w.write_bool(v.charts_settings.auto_arrange);
    w.write_bool(v.charts_settings.visible);
    w.write_bool(v.charts_settings.wide);
    w.write_bool(v.charts_settings.stay_on_top);
    w.write_i32(v.charts_settings.max_charts);
    w.write_i32(v.charts_settings.refresh);
    w.write_f64(v.charts_settings.x1);
    w.write_f64(v.charts_settings.x2);
    w.write_bool(v.spy_hide_mode);
    // HeatMapConfig
    w.write_bool(v.heatmap_config.show);
    w.write_bool(v.heatmap_config.use_q);
    w.write_f32(v.heatmap_config.height);
    w.write_bool(v.heatmap_config.cpu);
    w.write_bool(v.heatmap_config.trades);
    w.write_bool(v.heatmap_config.app_latency);
    w.write_bool(v.heatmap_config.draw_latency);
    w.write_i32(v.icon_selection);
    w.write_bool(v.remember_chart_buttons);
    w.write_bool(v.show_detects_tool);
    w.write_i32(v.scale_plus_index);
    w.write_i32(v.scale_minus_index);
    // NewsFormConfig
    let nf = &v.news_form_config;
    w.write_bool(nf.stay_on_top);
    w.write_bool(nf.exact_time);
    w.write_u8(nf.font_size);
    w.write_u8(nf.strength);
    w.write_bool(nf.update_orig);
    w.write_u8(nf.sound);
    w.write_i32(nf.theme);
    w.write_i32(nf.coin_card_font_size);
    w.write_bool(nf.full_tags);
    w.write_bool(nf.feed_mode);
    // FontSizes
    w.write_u8_array(&v.font_sizes);
    // Tail fields (always written, no gate needed on write side)
    w.write_u8(v.chart_candles_style);
    w.write_u8(v.chart_candles_tick_opacity);
    w.write_bool(v.chart_candles_neutral_ticks);
    w.write_u8(v.chart_candles_outline_width);
    w.write_bool(v.chart_candles_tick_wicks);
    for cc in &v.candle_colors {
        w.write_u32(cc.green);
        w.write_u32(cc.red);
        w.write_u32(cc.neutral);
    }
    w.write_bool(v.use_ai_coin_card);
    w.write_u8(v.ai_card_provider);
    w.write_string_x(&v.ai_card_model);
    w.write_string_x(&v.ai_card_prompt);
    w.write_bool(v.manual_charts_full_screen);
    w.write_bytes(&v.unknown_tail);
}

fn write_ini_sections(w: &mut Writer, sections: &[IniSectionData]) {
    if sections.len() > 2 {
        w.fail("too many shared config ini sections");
        return;
    }
    w.write_i32(sections.len() as i32);
    for sec in sections {
        w.write_string_x(&sec.name);
        if sec.entries.len() > MAX_INI_ENTRIES as usize {
            w.fail("too many shared config ini entries");
            return;
        }
        w.write_i32(sec.entries.len() as i32);
        for (k, v) in &sec.entries {
            w.write_string_x(k);
            w.write_string_x(v);
        }
    }
}

fn write_theme(w: &mut Writer, t: &ThemeSection) {
    w.write_u8(1);
    w.write_i32(t.current_style);
    write_ini_sections(w, &t.ini_sections);
    w.write_bytes(&t.unknown_tail);
}

fn write_ini(w: &mut Writer, i: &IniSection) {
    w.write_u8(1);
    write_ini_sections(w, &i.ini_sections);
    w.write_bytes(&i.unknown_tail);
}

fn write_hotkeys(w: &mut Writer, h: &HotkeysConfig) {
    w.write_bool(h.filled);
    w.write_u8(h.ver);
    w.write_f64_array(&h.o_size);
    w.write_i32(h.b_num);
    w.write_u16_array(&h.o_keys);
    w.write_u8(h.split_parts);
    w.write_u8(h.sb_num);
    w.write_u16_array(&h.s_keys);
    w.write_f32_array(&h.s_price);
    w.write_u16(h.cancel_buy);
    w.write_u16(h.panic_sell);
    w.write_u16(h.join_sells);
    w.write_u16(h.switch_charts);
    w.write_u16(h.reload_book);
    w.write_u16(h.new_long);
    w.write_u16(h.new_short);
    w.write_u16(h.split_order);
    w.write_u16(h.shift_buy_up);
    w.write_u16(h.shift_buy_down);
    w.write_u16(h.shift_sell_up);
    w.write_u16(h.shift_sell_down);
    w.write_u16(h.make_shot);
    w.write_u16(h.make_shot_bot);
    w.write_u16(h.reload_chart);
    w.write_u16(h.scale_plus);
    w.write_u16(h.scale_minus);
    w.write_u16(h.sell_plus);
    w.write_u16(h.sell_minus);
    w.write_u16(h.spy_mode);
    w.write_u16(h.show_charts);
    w.write_u16(h.split_order_x);
    w.write_u16(h.switch_figure);
    w.write_u16(h.fit_sells);
    w.write_u16(h.panic_sell_one);
    w.write_u16(h.cancel_all_buys);
    w.write_u16(h.broadcast);
}

fn write_ui(w: &mut Writer, u: &UiSection) {
    w.write_u8(3); // version
    w.write_bool(u.hide_demo_button);
    w.write_bool(u.confirm_close);
    w.write_bool(u.new_markets_on_top);
    w.write_i32(u.coins_sort_order);
    write_hotkeys(w, &u.hotkeys_config);
    w.write_string_x(&u.strat_editor_chapters);
    // MarketsTableConfig
    w.write_i32(u.markets_table_config.sort_col);
    w.write_bool_array(&u.markets_table_config.col_vis);
    w.write_u8_array(&u.markets_table_config.col_pos);
    w.write_u8(u.main_button_index_1);
    w.write_bool_array(&u.strat_expanded_state);
    w.write_bytes(&u.unknown_tail);
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a raw (uncompressed) shared-config payload into [`SharedConfig`].
pub fn parse_payload(data: &[u8]) -> Result<SharedConfig, SharedConfigError> {
    if data.len() > MAX_PAYLOAD_SIZE {
        return err("shared config payload too large");
    }
    let mut r = Reader::new(data);

    // Header
    r.need(HEADER_SIZE)?;
    let magic = r.read_bytes(4)?;
    if magic != MAGIC {
        return err("wrong shared config magic");
    }
    let version = r.read_u8()?;
    if version != VERSION {
        return err("unsupported shared config version");
    }
    let config_version = r.read_u16()?;

    let mut signals: Option<SignalsSection> = None;
    let mut trading: Option<TradingSection> = None;
    let mut visual: Option<VisualSection> = None;
    let mut theme: Option<ThemeSection> = None;
    let mut ini: Option<IniSection> = None;
    let mut ui: Option<UiSection> = None;
    let mut section_mask: u32 = 0;

    while let Some((kind, info)) = read_block_header(&mut r)? {
        // Create a sub-reader for this block's body.
        let body = &r.data[info.body_start..info.block_end];
        let mut br = Reader::new(body);

        match kind {
            KIND_SIGNALS => {
                signals = Some(parse_signals(&mut br, body.len())?);
                section_mask |= 1 << KIND_SIGNALS;
            }
            KIND_TRADING => {
                trading = Some(parse_trading(&mut br, body.len())?);
                section_mask |= 1 << KIND_TRADING;
            }
            KIND_VISUAL => {
                visual = Some(parse_visual(&mut br, body.len())?);
                section_mask |= 1 << KIND_VISUAL;
            }
            KIND_THEME => {
                theme = Some(parse_theme(&mut br, body.len())?);
                section_mask |= 1 << KIND_THEME;
            }
            KIND_INI => {
                ini = Some(parse_ini(&mut br, body.len())?);
                section_mask |= 1 << KIND_INI;
            }
            KIND_UI => {
                ui = Some(parse_ui(&mut br, body.len())?);
                section_mask |= 1 << KIND_UI;
            }
            _ => { /* skip unknown block */ }
        }

        // Check the sub-reader didn't overrun.
        if br.pos > body.len() {
            return err("wrong shared config block data");
        }

        // Advance the outer reader past this block.
        r.pos = info.block_end;
    }

    if section_mask != REQUIRED_MASK {
        return err("incomplete shared config");
    }

    Ok(SharedConfig {
        config_version,
        signals: signals.unwrap(),
        trading: trading.unwrap(),
        visual: visual.unwrap(),
        theme: theme.unwrap(),
        ini: ini.unwrap(),
        ui: ui.unwrap(),
    })
}

/// Serialize a [`SharedConfig`] into a raw (uncompressed) payload.
///
/// Length limits are checked while writing, before a section can grow beyond
/// the bounds accepted by the core reader.
pub fn serialize_payload(cfg: &SharedConfig) -> Result<Vec<u8>, SharedConfigError> {
    let mut w = Writer::new();

    // Header
    w.write_bytes(MAGIC);
    w.write_u8(VERSION);
    w.write_u16(cfg.config_version);

    // Signals
    let sp = begin_block(&mut w, KIND_SIGNALS);
    write_signals(&mut w, &cfg.signals);
    end_block(&mut w, sp);

    // Trading
    let sp = begin_block(&mut w, KIND_TRADING);
    write_trading(&mut w, &cfg.trading);
    end_block(&mut w, sp);

    // Visual
    let sp = begin_block(&mut w, KIND_VISUAL);
    write_visual(&mut w, &cfg.visual);
    end_block(&mut w, sp);

    // Theme
    let sp = begin_block(&mut w, KIND_THEME);
    write_theme(&mut w, &cfg.theme);
    end_block(&mut w, sp);

    // Ini
    let sp = begin_block(&mut w, KIND_INI);
    write_ini(&mut w, &cfg.ini);
    end_block(&mut w, sp);

    // Ui
    let sp = begin_block(&mut w, KIND_UI);
    write_ui(&mut w, &cfg.ui);
    end_block(&mut w, sp);

    w.finish()
}
