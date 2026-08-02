//! Typed report-DB replication state.

use crate::commands::registry::decode_utf8_delphi;
use crate::commands::report::{RepSchema as WireSchema, RepSyncPage as WireSyncPage};
use crate::compression::{rlelz_decompress_exact, synlz_decompress};
use std::collections::HashSet;
use std::fmt;
use std::sync::Arc;

const REPORT_SCHEMA_FORMAT_VERSION: u8 = 1;
const REPORT_TEXT_MAX_BYTES: usize = 8192;
const REPORT_REC_ID_FIELD_NAME: &str = "newRecID";
const REPORT_DELETED_FIELD_NAME: &str = "deleted";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportFieldKind {
    Integer,
    Float,
    Text,
}

impl ReportFieldKind {
    fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Integer),
            2 => Some(Self::Float),
            3 => Some(Self::Text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSchemaField {
    pub index: u16,
    pub name: String,
    pub kind: ReportFieldKind,
    pub sql_spec: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSchema {
    format_version: u8,
    fields: Arc<[ReportSchemaField]>,
    rec_id_field_index: u16,
}

impl ReportSchema {
    pub fn format_version(&self) -> u8 {
        self.format_version
    }

    /// Append-only field count. It also serves as the schema revision.
    pub fn revision(&self) -> usize {
        self.fields.len()
    }

    pub fn fields(&self) -> &[ReportSchemaField] {
        &self.fields
    }

    pub fn field(&self, index: u16) -> Option<&ReportSchemaField> {
        self.fields.get(usize::from(index))
    }

    pub fn field_by_name(&self, name: &str) -> Option<&ReportSchemaField> {
        self.fields.iter().find(|field| field.name == name)
    }

    pub fn rec_id_field_index(&self) -> u16 {
        self.rec_id_field_index
    }

    /// Build a SQLite table with `newRecID` as its stable primary key.
    ///
    /// `sql_spec` comes from the authenticated MoonBot core schema. Identifiers
    /// are quoted locally; the helper does not open or own an application DB.
    pub fn sqlite_create_table_sql(&self, table: &str) -> String {
        let table = quote_sqlite_identifier(table);
        let mut columns = self
            .fields
            .iter()
            .map(|field| {
                format!(
                    "{} {}",
                    quote_sqlite_identifier(&field.name),
                    field.sql_spec
                )
            })
            .collect::<Vec<_>>();
        columns.push(format!(
            "PRIMARY KEY ({})",
            quote_sqlite_identifier(REPORT_REC_ID_FIELD_NAME)
        ));
        format!("CREATE TABLE {table} ({})", columns.join(", "))
    }

    pub fn sqlite_add_column_sql(&self, table: &str, field: &ReportSchemaField) -> String {
        format!(
            "ALTER TABLE {} ADD COLUMN {} {}",
            quote_sqlite_identifier(table),
            quote_sqlite_identifier(&field.name),
            field.sql_spec
        )
    }

    pub fn sqlite_unique_index_sql(&self, table: &str) -> String {
        let index_name = format!("I{}_newRecID", table);
        format!(
            "CREATE UNIQUE INDEX IF NOT EXISTS {} ON {} ({})",
            quote_sqlite_identifier(&index_name),
            quote_sqlite_identifier(table),
            quote_sqlite_identifier(REPORT_REC_ID_FIELD_NAME)
        )
    }

    fn parse_compressed(data: &[u8]) -> Option<Self> {
        let decoded = synlz_decompress(data)?;
        let mut pos = 0usize;
        let format_version = read_u8(&decoded, &mut pos)?;
        if format_version != REPORT_SCHEMA_FORMAT_VERSION {
            return None;
        }
        let field_count = usize::from(read_u16(&decoded, &mut pos)?);
        let mut fields = Vec::new();
        fields.try_reserve_exact(field_count).ok()?;
        let mut names = HashSet::new();
        names.try_reserve(field_count).ok()?;
        let mut rec_id_field_index = None;
        for index in 0..field_count {
            let name = read_str8(&decoded, &mut pos)?;
            if !names.insert(name.clone()) {
                return None;
            }
            let kind = ReportFieldKind::from_wire(read_u8(&decoded, &mut pos)?)?;
            let sql_spec = read_str8(&decoded, &mut pos)?;
            let index = u16::try_from(index).ok()?;
            if name == REPORT_REC_ID_FIELD_NAME {
                if kind != ReportFieldKind::Integer {
                    return None;
                }
                rec_id_field_index = Some(index);
            }
            fields.push(ReportSchemaField {
                index,
                name,
                kind,
                sql_spec,
            });
        }
        if pos != decoded.len() {
            return None;
        }
        Some(Self {
            format_version,
            fields: fields.into(),
            rec_id_field_index: rec_id_field_index?,
        })
    }

    fn is_append_only_successor_of(&self, previous: &Self) -> bool {
        self.fields.len() >= previous.fields.len()
            && self
                .fields
                .iter()
                .zip(previous.fields.iter())
                .all(|(new, old)| {
                    new.index == old.index
                        && new.name == old.name
                        && new.kind == old.kind
                        && new.sql_spec == old.sql_spec
                })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReportValue {
    Integer(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportFieldValue {
    pub field_index: u16,
    pub value: ReportValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportRow {
    pub rec_id: i64,
    pub fields: Vec<ReportFieldValue>,
}

impl ReportRow {
    pub fn value(&self, field_index: u16) -> Option<&ReportValue> {
        self.fields
            .binary_search_by_key(&field_index, |field| field.field_index)
            .ok()
            .map(|index| &self.fields[index].value)
    }

    pub fn value_by_name<'a>(
        &'a self,
        schema: &ReportSchema,
        name: &str,
    ) -> Option<&'a ReportValue> {
        self.value(schema.field_by_name(name)?.index)
    }

    pub fn integer_by_name(&self, schema: &ReportSchema, name: &str) -> Option<i64> {
        match self.value_by_name(schema, name)? {
            ReportValue::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn float_by_name(&self, schema: &ReportSchema, name: &str) -> Option<f64> {
        match self.value_by_name(schema, name)? {
            ReportValue::Float(value) => Some(*value),
            _ => None,
        }
    }

    pub fn text_by_name<'a>(&'a self, schema: &ReportSchema, name: &str) -> Option<&'a str> {
        match self.value_by_name(schema, name)? {
            ReportValue::Text(value) => Some(value),
            _ => None,
        }
    }

    fn set_integer(&mut self, field_index: u16, value: i64) -> bool {
        let Ok(index) = self
            .fields
            .binary_search_by_key(&field_index, |field| field.field_index)
        else {
            return false;
        };
        let ReportValue::Integer(current) = &mut self.fields[index].value else {
            return false;
        };
        *current = value;
        true
    }

    fn parse(data: &[u8], rec_id_field_index: u16, outer_rec_id: Option<i64>) -> Option<Self> {
        let mut pos = 0usize;
        let row = Self::parse_from(data, &mut pos, rec_id_field_index, outer_rec_id)?;
        (pos == data.len()).then_some(row)
    }

    fn parse_from(
        data: &[u8],
        pos: &mut usize,
        rec_id_field_index: u16,
        outer_rec_id: Option<i64>,
    ) -> Option<Self> {
        let field_count = usize::from(read_u16(data, pos)?);
        let mut fields = Vec::new();
        fields.try_reserve_exact(field_count).ok()?;
        let mut row_rec_id = None;
        for _ in 0..field_count {
            let field_index = read_u16(data, pos)?;
            let kind = ReportFieldKind::from_wire(read_u8(data, pos)?)?;
            let value = match kind {
                ReportFieldKind::Integer => ReportValue::Integer(read_i64(data, pos)?),
                ReportFieldKind::Float => ReportValue::Float(read_f64(data, pos)?),
                ReportFieldKind::Text => {
                    let len = usize::from(read_u16(data, pos)?);
                    if len > REPORT_TEXT_MAX_BYTES {
                        return None;
                    }
                    let end = pos.checked_add(len)?;
                    if end > data.len() {
                        return None;
                    }
                    let value = decode_utf8_delphi(&data[*pos..end]);
                    *pos = end;
                    ReportValue::Text(value)
                }
            };
            if field_index == rec_id_field_index {
                let ReportValue::Integer(value) = value else {
                    return None;
                };
                row_rec_id = Some(value);
                fields.push(ReportFieldValue {
                    field_index,
                    value: ReportValue::Integer(value),
                });
            } else {
                fields.push(ReportFieldValue { field_index, value });
            }
        }
        fields.sort_unstable_by_key(|field| field.field_index);
        if fields
            .windows(2)
            .any(|pair| pair[0].field_index == pair[1].field_index)
        {
            return None;
        }
        let rec_id = outer_rec_id.or(row_rec_id)?;
        if rec_id <= 0 || row_rec_id.is_some_and(|row| row != rec_id) {
            return None;
        }
        Some(Self { rec_id, fields })
    }

    fn parse_many(data: &[u8], count: u16, rec_id_field_index: u16) -> Option<Vec<Self>> {
        let mut pos = 0usize;
        let mut rows = Vec::new();
        rows.try_reserve_exact(usize::from(count)).ok()?;
        for _ in 0..count {
            rows.push(Self::parse_from(data, &mut pos, rec_id_field_index, None)?);
        }
        if pos != data.len() {
            return None;
        }
        Some(rows)
    }
}

/// Inclusive `newRecID` range used by report soft-delete/restore intents.
///
/// A reversed range (`from_rec_id > to_rec_id`) is valid and selects no rows,
/// matching the core's SQL `BETWEEN` semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReportRecIdRange {
    pub from_rec_id: i64,
    pub to_rec_id: i64,
}

impl ReportRecIdRange {
    pub const fn new(from_rec_id: i64, to_rec_id: i64) -> Self {
        Self {
            from_rec_id,
            to_rec_id,
        }
    }

    pub const fn contains(self, rec_id: i64) -> bool {
        self.from_rec_id <= rec_id && rec_id <= self.to_rec_id
    }
}

/// One server-applied report soft-delete/restore operation.
///
/// The same value is used for outbound intent batches and inbound server echoes.
/// An echo means the core committed the update to its report database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportRowsDeleted {
    pub deleted: bool,
    pub ranges: Arc<[ReportRecIdRange]>,
    pub singles: Arc<[i64]>,
}

impl ReportRowsDeleted {
    pub fn new(
        deleted: bool,
        ranges: impl IntoIterator<Item = ReportRecIdRange>,
        singles: impl IntoIterator<Item = i64>,
    ) -> Self {
        Self {
            deleted,
            ranges: ranges.into_iter().collect::<Vec<_>>().into(),
            singles: singles.into_iter().collect::<Vec<_>>().into(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty() && self.singles.is_empty()
    }

    pub fn affects(&self, rec_id: i64) -> bool {
        self.ranges.iter().any(|range| range.contains(rec_id)) || self.singles.contains(&rec_id)
    }

    pub(crate) fn wire_batches(&self) -> Vec<Self> {
        const HEADER_AND_COUNTS_BYTES: usize = 11 + 1 + 2 + 2;
        const RANGE_BYTES: usize = 2 * std::mem::size_of::<i64>();
        const SINGLE_BYTES: usize = std::mem::size_of::<i64>();

        if self.is_empty() {
            return Vec::new();
        }

        let mut batches = Vec::new();
        let mut ranges = Vec::new();
        let mut singles = Vec::new();
        let mut wire_bytes = HEADER_AND_COUNTS_BYTES;

        let flush = |batches: &mut Vec<Self>,
                     ranges: &mut Vec<ReportRecIdRange>,
                     singles: &mut Vec<i64>| {
            if ranges.is_empty() && singles.is_empty() {
                return;
            }
            batches.push(Self {
                deleted: self.deleted,
                ranges: std::mem::take(ranges).into(),
                singles: std::mem::take(singles).into(),
            });
        };

        for range in self.ranges.iter().copied() {
            if wire_bytes + RANGE_BYTES > crate::commands::report::MAX_SET_ROWS_DELETED_WIRE_BYTES {
                flush(&mut batches, &mut ranges, &mut singles);
                wire_bytes = HEADER_AND_COUNTS_BYTES;
            }
            ranges.push(range);
            wire_bytes += RANGE_BYTES;
        }
        for rec_id in self.singles.iter().copied() {
            if wire_bytes + SINGLE_BYTES > crate::commands::report::MAX_SET_ROWS_DELETED_WIRE_BYTES
            {
                flush(&mut batches, &mut ranges, &mut singles);
                wire_bytes = HEADER_AND_COUNTS_BYTES;
            }
            singles.push(rec_id);
            wire_bytes += SINGLE_BYTES;
        }
        flush(&mut batches, &mut ranges, &mut singles);
        batches
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportHistoryDepth {
    ServerDefault,
    Days(u16),
    All,
}

impl ReportHistoryDepth {
    pub(crate) fn to_wire(self) -> u16 {
        match self {
            Self::ServerDefault => 0,
            Self::Days(days) => days,
            Self::All => u16::MAX,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportSyncRequest {
    pub from_rec_id: i64,
    pub history_depth: ReportHistoryDepth,
}

impl ReportSyncRequest {
    pub fn fresh(history_depth: ReportHistoryDepth) -> Self {
        Self {
            from_rec_id: 0,
            history_depth,
        }
    }

    pub fn resume(from_rec_id: i64) -> Self {
        Self {
            from_rec_id,
            history_depth: ReportHistoryDepth::ServerDefault,
        }
    }

    pub(crate) fn is_valid(self) -> bool {
        self.from_rec_id >= 0
            && !matches!(self.history_depth, ReportHistoryDepth::Days(0 | u16::MAX))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReportSyncTicket {
    pub sync_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReportSyncPage {
    pub ticket: ReportSyncTicket,
    pub request_uid: u64,
    /// Stable identity of the core report database used for this page.
    pub epoch: i32,
    pub from_rec_id: i64,
    pub last_rec_id: i64,
    pub max_rec_id: i64,
    pub rows: Arc<[ReportRow]>,
    pub database_recreated: bool,
    wire_row_count: u16,
}

impl ReportSyncPage {
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn is_complete(&self) -> bool {
        !self.database_recreated
            && (self.wire_row_count == 0 || self.last_rec_id >= self.max_rec_id)
    }

    /// Number of rows declared by the server before live-wins filtering.
    pub fn source_row_count(&self) -> u16 {
        self.wire_row_count
    }

    pub fn next_from_rec_id(&self) -> Option<i64> {
        (!self.database_recreated && !self.is_complete())
            .then(|| self.last_rec_id.saturating_add(1))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportSyncComplete {
    pub ticket: ReportSyncTicket,
    pub page_count: u32,
    pub total_rows: u32,
    /// Stable identity of the core report database used for this catch-up.
    pub epoch: i32,
    pub max_rec_id: i64,
    pub next_from_rec_id: i64,
}

/// Durable report-replica cursor stored atomically with the application's rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportSyncCheckpoint {
    /// Stable identity of the core report database.
    pub epoch: i32,
    /// First `newRecID` not yet requested from this database.
    pub next_from_rec_id: i64,
}

impl ReportSyncCheckpoint {
    pub(crate) fn is_valid(self) -> bool {
        self.epoch != 0 && self.next_from_rec_id >= 0
    }
}

impl ReportSyncComplete {
    /// Return the durable cursor to commit with the local report replica.
    pub const fn checkpoint(&self) -> ReportSyncCheckpoint {
        ReportSyncCheckpoint {
            epoch: self.epoch,
            next_from_rec_id: self.next_from_rec_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReportAliveMapRequest {
    pub epoch: i32,
    pub up_to_rec_id: i64,
}

impl ReportAliveMapRequest {
    pub(crate) fn is_valid(self) -> bool {
        self.epoch != 0 && self.up_to_rec_id >= 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReportAliveMapTicket {
    /// Client-local operation identifier used to match completion events.
    pub sync_id: u64,
}

/// Result of reconciling row visibility after normal report catch-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportAliveMapOutcome {
    /// The core uses another report database. Clear the local replica and sync it again.
    DatabaseRecreated,
    /// Complete alive state for `newRecID=1..=covered_up_to` is available.
    Snapshot,
}

#[derive(Clone, PartialEq, Eq)]
pub struct ReportAliveMapComplete {
    /// Client-local operation identifier returned by `reconcile_alive`.
    pub ticket: ReportAliveMapTicket,
    /// Core report-database identity returned with the map.
    pub epoch: i32,
    /// Highest `newRecID` covered by this map.
    pub covered_up_to: i64,
    /// Whether the map is usable or belongs to another database.
    pub outcome: ReportAliveMapOutcome,
    bitmap: Option<Arc<[u8]>>,
}

impl std::fmt::Debug for ReportAliveMapComplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReportAliveMapComplete")
            .field("ticket", &self.ticket)
            .field("epoch", &self.epoch)
            .field("covered_up_to", &self.covered_up_to)
            .field("outcome", &self.outcome)
            .field("bitmap_len", &self.bitmap.as_deref().map(<[u8]>::len))
            .finish()
    }
}

impl ReportAliveMapComplete {
    /// Return whether a row exists and has `deleted=0` in the authoritative database.
    pub fn is_alive(&self, rec_id: i64) -> Option<bool> {
        if self.outcome != ReportAliveMapOutcome::Snapshot {
            return None;
        }
        let bitmap = self.bitmap.as_ref()?;
        if rec_id <= 0 || rec_id > self.covered_up_to {
            return None;
        }
        let bit = usize::try_from(rec_id - 1).ok()?;
        Some(bitmap.get(bit / 8)? & (1 << (bit % 8)) != 0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReportEvent {
    Schema(Arc<ReportSchema>),
    RowUpsert(ReportRow),
    RowDelete {
        rec_id: i64,
    },
    /// Soft-delete/restore committed by the core and broadcast to all report subscribers.
    RowsDeleted(ReportRowsDeleted),
    SyncStarted {
        ticket: ReportSyncTicket,
        request: ReportSyncRequest,
    },
    SyncPage(Arc<ReportSyncPage>),
    SyncComplete(ReportSyncComplete),
    AliveMapComplete(ReportAliveMapComplete),
    OpenRowsCheckStarted {
        rec_ids: Arc<[i64]>,
    },
    OpenRowsCheckComplete {
        rec_ids: Arc<[i64]>,
    },
    SchemaRejected {
        reason: String,
    },
}

#[derive(Debug)]
struct ActiveSync {
    ticket: ReportSyncTicket,
    initial_history_depth: ReportHistoryDepth,
    current_request: ReportSyncRequest,
    current_request_uid: u64,
    expected_epoch: Option<i32>,
    current_epoch: Option<i32>,
    page_count: u32,
    total_rows: u32,
    awaiting_apply: Option<Arc<ReportSyncPage>>,
    live_touched: HashSet<i64>,
    deleted_overrides: Vec<ReportRowsDeleted>,
}

#[derive(Debug)]
struct ActiveAliveMap {
    ticket: ReportAliveMapTicket,
    request: ReportAliveMapRequest,
    request_uid: u64,
    overrides: Vec<ReportAliveOverride>,
}

#[derive(Debug)]
enum ReportAliveOverride {
    RowsDeleted(ReportRowsDeleted),
    Row { rec_id: i64, alive: bool },
}

fn report_alive_bitmap_len(up_to_rec_id: i64) -> Option<usize> {
    let bits = up_to_rec_id.checked_add(7)? / 8;
    usize::try_from(bits).ok()
}

fn report_alive_tail_is_zero(bitmap: &[u8], up_to_rec_id: i64) -> bool {
    let used_bits = u32::try_from(up_to_rec_id.rem_euclid(8)).unwrap_or(0);
    if used_bits == 0 || bitmap.is_empty() {
        return true;
    }
    let used_mask = (1u8 << used_bits) - 1;
    bitmap.last().is_some_and(|last| last & !used_mask == 0)
}

fn set_alive_bit(bitmap: &mut [u8], rec_id: i64, alive: bool) {
    let Ok(bit) = usize::try_from(rec_id - 1) else {
        return;
    };
    let Some(byte) = bitmap.get_mut(bit / 8) else {
        return;
    };
    let mask = 1 << (bit % 8);
    if alive {
        *byte |= mask;
    } else {
        *byte &= !mask;
    }
}

fn set_alive_range(
    bitmap: &mut [u8],
    up_to_rec_id: i64,
    mut from_rec_id: i64,
    mut to_rec_id: i64,
    alive: bool,
) {
    from_rec_id = from_rec_id.max(1);
    to_rec_id = to_rec_id.min(up_to_rec_id);
    if from_rec_id > to_rec_id {
        return;
    }
    while from_rec_id <= to_rec_id && (from_rec_id - 1) % 8 != 0 {
        set_alive_bit(bitmap, from_rec_id, alive);
        from_rec_id += 1;
    }
    let full_bytes = (to_rec_id - from_rec_id + 1) / 8;
    if full_bytes > 0 {
        let Ok(first_byte) = usize::try_from((from_rec_id - 1) / 8) else {
            return;
        };
        let Ok(byte_count) = usize::try_from(full_bytes) else {
            return;
        };
        let Some(end) = first_byte.checked_add(byte_count) else {
            return;
        };
        let Some(bytes) = bitmap.get_mut(first_byte..end) else {
            return;
        };
        bytes.fill(if alive { u8::MAX } else { 0 });
        from_rec_id += full_bytes * 8;
    }
    while from_rec_id <= to_rec_id {
        set_alive_bit(bitmap, from_rec_id, alive);
        from_rec_id += 1;
    }
}

fn apply_alive_override(bitmap: &mut [u8], up_to_rec_id: i64, change: &ReportAliveOverride) {
    match change {
        ReportAliveOverride::RowsDeleted(change) => {
            for range in change.ranges.iter() {
                set_alive_range(
                    bitmap,
                    up_to_rec_id,
                    range.from_rec_id,
                    range.to_rec_id,
                    !change.deleted,
                );
            }
            for &rec_id in change.singles.iter() {
                if rec_id > 0 && rec_id <= up_to_rec_id {
                    set_alive_bit(bitmap, rec_id, !change.deleted);
                }
            }
        }
        ReportAliveOverride::Row { rec_id, alive } => {
            if *rec_id > 0 && *rec_id <= up_to_rec_id {
                set_alive_bit(bitmap, *rec_id, *alive);
            }
        }
    }
}

impl ActiveSync {
    fn new(
        ticket: ReportSyncTicket,
        request: ReportSyncRequest,
        request_uid: u64,
        expected_epoch: Option<i32>,
    ) -> Self {
        Self {
            ticket,
            initial_history_depth: request.history_depth,
            current_request: request,
            current_request_uid: request_uid,
            expected_epoch,
            current_epoch: None,
            page_count: 0,
            total_rows: 0,
            awaiting_apply: None,
            live_touched: HashSet::new(),
            deleted_overrides: Vec::new(),
        }
    }
}

#[derive(Debug)]
struct ActiveOpenRowsCheck {
    rec_ids: Arc<[i64]>,
    pending: HashSet<i64>,
}

impl ActiveOpenRowsCheck {
    fn new(rec_ids: Arc<[i64]>) -> Self {
        Self {
            pending: rec_ids.iter().copied().collect(),
            rec_ids,
        }
    }
}

#[derive(Debug)]
pub(crate) enum ReportPageApplyAction {
    SendNext {
        request_uid: u64,
        request: ReportSyncRequest,
        expected_epoch: Option<i32>,
    },
    Complete {
        received_request_uid: u64,
        durable_request: ReportSyncRequest,
        epoch: i32,
    },
    Ignored,
}

#[derive(Debug)]
pub(crate) enum ReportControl {
    SendSync {
        request_uid: u64,
        request: ReportSyncRequest,
    },
    PageReceived {
        request_uid: u64,
    },
    SendOpenRowsCheck {
        rec_ids: Arc<[i64]>,
    },
    SchemaReceived,
    OpenRowsCheckCompleted,
    AliveMapReceived {
        request_uid: u64,
    },
}

#[derive(Default)]
pub(crate) struct ReportReplicationState {
    schema: Option<Arc<ReportSchema>>,
    pending_after_schema: Option<(ReportSyncTicket, ReportSyncRequest, Option<i32>)>,
    pending_check_after_schema: Option<Arc<[i64]>>,
    active: Option<ActiveSync>,
    active_check: Option<ActiveOpenRowsCheck>,
    active_alive_map: Option<ActiveAliveMap>,
}

impl ReportReplicationState {
    pub(crate) fn schema(&self) -> Option<&Arc<ReportSchema>> {
        self.schema.as_ref()
    }

    pub(crate) fn defer_sync_until_schema(
        &mut self,
        ticket: ReportSyncTicket,
        request: ReportSyncRequest,
        expected_epoch: Option<i32>,
    ) {
        self.pending_after_schema = Some((ticket, request, expected_epoch));
        self.active = None;
    }

    pub(crate) fn begin_sync(
        &mut self,
        ticket: ReportSyncTicket,
        request: ReportSyncRequest,
        expected_epoch: Option<i32>,
        out: &mut Vec<ReportEvent>,
    ) -> u64 {
        let request_uid = random_nonzero_u64();
        self.pending_after_schema = None;
        self.active = Some(ActiveSync::new(
            ticket,
            request,
            request_uid,
            expected_epoch,
        ));
        out.push(ReportEvent::SyncStarted { ticket, request });
        request_uid
    }

    pub(crate) fn waiting_for_page_apply(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.awaiting_apply.is_some())
    }

    pub(crate) fn retry_active_page(&mut self) -> Option<(u64, ReportSyncRequest)> {
        let active = self.active.as_mut()?;
        if active.awaiting_apply.is_some() {
            return None;
        }
        Some((active.current_request_uid, active.current_request))
    }

    pub(crate) fn begin_alive_map(
        &mut self,
        ticket: ReportAliveMapTicket,
        request: ReportAliveMapRequest,
    ) -> u64 {
        let request_uid = random_nonzero_u64();
        self.active_alive_map = Some(ActiveAliveMap {
            ticket,
            request,
            request_uid,
            overrides: Vec::new(),
        });
        request_uid
    }

    pub(crate) fn retry_active_alive_map(&self) -> Option<(u64, ReportAliveMapRequest)> {
        let active = self.active_alive_map.as_ref()?;
        Some((active.request_uid, active.request))
    }

    pub(crate) fn defer_open_rows_check_until_schema(&mut self, rec_ids: Arc<[i64]>) {
        self.pending_check_after_schema = Some(rec_ids);
        self.active_check = None;
    }

    pub(crate) fn begin_open_rows_check(
        &mut self,
        rec_ids: Arc<[i64]>,
        out: &mut Vec<ReportEvent>,
    ) {
        self.pending_check_after_schema = None;
        self.active_check = Some(ActiveOpenRowsCheck::new(Arc::clone(&rec_ids)));
        out.push(ReportEvent::OpenRowsCheckStarted { rec_ids });
    }

    pub(crate) fn clear_open_rows_check(&mut self) {
        self.pending_check_after_schema = None;
        self.active_check = None;
    }

    pub(crate) fn pending_open_row_ids(&self) -> Option<Arc<[i64]>> {
        let check = self.active_check.as_ref()?;
        let mut ids = check.pending.iter().copied().collect::<Vec<_>>();
        ids.sort_unstable();
        Some(ids.into())
    }

    pub(crate) fn apply_schema(
        &mut self,
        wire: WireSchema,
        out: &mut Vec<ReportEvent>,
        controls: &mut Vec<ReportControl>,
    ) -> bool {
        let Some(schema) = ReportSchema::parse_compressed(&wire.data) else {
            return false;
        };
        if let Some(previous) = self.schema.as_deref() {
            if !schema.is_append_only_successor_of(previous) {
                out.push(ReportEvent::SchemaRejected {
                    reason: "schema changed an existing field; append-only invariant violated"
                        .to_string(),
                });
                return true;
            }
        }
        let schema = Arc::new(schema);
        self.schema = Some(Arc::clone(&schema));
        out.push(ReportEvent::Schema(schema));
        controls.push(ReportControl::SchemaReceived);
        if let Some((ticket, request, expected_epoch)) = self.pending_after_schema.take() {
            let request_uid = self.begin_sync(ticket, request, expected_epoch, out);
            controls.push(ReportControl::SendSync {
                request_uid,
                request,
            });
        }
        if let Some(rec_ids) = self.pending_check_after_schema.take() {
            self.begin_open_rows_check(Arc::clone(&rec_ids), out);
            controls.push(ReportControl::SendOpenRowsCheck { rec_ids });
        }
        true
    }

    pub(crate) fn apply_live_upsert(
        &mut self,
        rec_id: i64,
        raw: &[u8],
        out: &mut Vec<ReportEvent>,
        controls: &mut Vec<ReportControl>,
    ) -> bool {
        let Some(schema) = self.schema.as_deref() else {
            // Asking for the schema also enables live delivery on the server.
            // A live row may overtake that sliced schema on UDP. The deferred
            // catch-up starts after schema validation and carries the current
            // row, so publishing an unmappable row or buffering it is unnecessary.
            return true;
        };
        let Some(row) = ReportRow::parse(raw, schema.rec_id_field_index, Some(rec_id)) else {
            return false;
        };
        let alive = row
            .integer_by_name(schema, REPORT_DELETED_FIELD_NAME)
            .unwrap_or(0)
            == 0;
        if let Some(active) = self.active.as_mut() {
            active.live_touched.insert(rec_id);
        }
        if let Some(active) = self.active_alive_map.as_mut() {
            active
                .overrides
                .push(ReportAliveOverride::Row { rec_id, alive });
        }
        out.push(ReportEvent::RowUpsert(row));
        self.resolve_open_row_check(rec_id, out, controls);
        true
    }

    pub(crate) fn apply_live_delete(
        &mut self,
        rec_id: i64,
        out: &mut Vec<ReportEvent>,
        controls: &mut Vec<ReportControl>,
    ) -> bool {
        if rec_id <= 0 {
            return false;
        }
        if self.schema.is_none() {
            return true;
        }
        if let Some(active) = self.active.as_mut() {
            active.live_touched.insert(rec_id);
        }
        if let Some(active) = self.active_alive_map.as_mut() {
            active.overrides.push(ReportAliveOverride::Row {
                rec_id,
                alive: false,
            });
        }
        out.push(ReportEvent::RowDelete { rec_id });
        self.resolve_open_row_check(rec_id, out, controls);
        true
    }

    pub(crate) fn apply_rows_deleted(
        &mut self,
        change: ReportRowsDeleted,
        out: &mut Vec<ReportEvent>,
    ) {
        if let Some(active) = self.active.as_mut() {
            active.deleted_overrides.push(change.clone());
        }
        if let Some(active) = self.active_alive_map.as_mut() {
            active
                .overrides
                .push(ReportAliveOverride::RowsDeleted(change.clone()));
        }
        out.push(ReportEvent::RowsDeleted(change));
    }

    pub(crate) fn apply_alive_map(
        &mut self,
        wire: crate::commands::report::RepAliveMap,
        out: &mut Vec<ReportEvent>,
        controls: &mut Vec<ReportControl>,
    ) -> bool {
        let Some(active) = self.active_alive_map.as_ref() else {
            return true;
        };
        if wire.request_uid != active.request_uid {
            return true;
        }
        if wire.epoch == 0 || wire.covered_up_to < 0 {
            return false;
        }

        let request = active.request;
        let database_recreated = request.epoch != wire.epoch;
        if !database_recreated && wire.covered_up_to != request.up_to_rec_id {
            return false;
        }

        let bitmap = if database_recreated {
            None
        } else {
            let Some(expected_len) = report_alive_bitmap_len(wire.covered_up_to) else {
                return false;
            };
            let mut bitmap = if expected_len == 0 {
                if !wire.data.is_empty() {
                    return false;
                }
                Vec::new()
            } else {
                let Some(bitmap) = rlelz_decompress_exact(&wire.data, expected_len) else {
                    return false;
                };
                bitmap
            };
            if !report_alive_tail_is_zero(&bitmap, wire.covered_up_to) {
                return false;
            }
            for change in &active.overrides {
                apply_alive_override(&mut bitmap, wire.covered_up_to, change);
            }
            Some(bitmap.into())
        };

        let active = self
            .active_alive_map
            .take()
            .expect("active alive map checked above");
        let outcome = if database_recreated {
            ReportAliveMapOutcome::DatabaseRecreated
        } else {
            ReportAliveMapOutcome::Snapshot
        };
        out.push(ReportEvent::AliveMapComplete(ReportAliveMapComplete {
            ticket: active.ticket,
            epoch: wire.epoch,
            covered_up_to: wire.covered_up_to,
            outcome,
            bitmap,
        }));
        controls.push(ReportControl::AliveMapReceived {
            request_uid: wire.request_uid,
        });
        true
    }

    pub(crate) fn apply_sync_page(
        &mut self,
        wire: WireSyncPage,
        out: &mut Vec<ReportEvent>,
        controls: &mut Vec<ReportControl>,
    ) -> bool {
        let Some(schema) = self.schema.as_deref() else {
            return false;
        };
        let Some(active) = self.active.as_mut() else {
            return true;
        };
        if wire.request_uid != active.current_request_uid {
            return true;
        }
        if active.awaiting_apply.is_some() {
            return true;
        }
        if wire.epoch == 0 {
            return false;
        }
        let epoch_recreated = active
            .current_epoch
            .or(active.expected_epoch)
            .is_some_and(|epoch| epoch != wire.epoch);
        let mut rows = if wire.row_count == 0 {
            if wire.last_rec_id != 0 || !wire.blob.is_empty() {
                return false;
            }
            Vec::new()
        } else {
            let Some(decoded) = synlz_decompress(&wire.blob) else {
                return false;
            };
            let Some(rows) =
                ReportRow::parse_many(&decoded, wire.row_count, schema.rec_id_field_index)
            else {
                return false;
            };
            rows
        };
        if wire.max_rec_id < 0
            || rows.windows(2).any(|pair| pair[0].rec_id >= pair[1].rec_id)
            || rows
                .last()
                .is_some_and(|row| row.rec_id != wire.last_rec_id)
            || (active.current_request.from_rec_id > 0
                && rows
                    .first()
                    .is_some_and(|row| row.rec_id < active.current_request.from_rec_id))
        {
            return false;
        }
        let highwater_recreated = active.current_request.from_rec_id > 0
            && wire.max_rec_id < active.current_request.from_rec_id.saturating_sub(1);
        let database_recreated = epoch_recreated || highwater_recreated;
        if highwater_recreated && !epoch_recreated && !rows.is_empty() {
            return false;
        }
        if !database_recreated && wire.last_rec_id > wire.max_rec_id {
            return false;
        }
        if database_recreated {
            rows.clear();
        } else {
            active.current_epoch = Some(wire.epoch);
        }
        if let Some(deleted_field) = schema
            .field_by_name(REPORT_DELETED_FIELD_NAME)
            .filter(|field| field.kind == ReportFieldKind::Integer)
        {
            for row in &mut rows {
                if let Some(change) = active
                    .deleted_overrides
                    .iter()
                    .rev()
                    .find(|change| change.affects(row.rec_id))
                {
                    row.set_integer(deleted_field.index, if change.deleted { 1 } else { 0 });
                }
            }
        }
        rows.retain(|row| !active.live_touched.contains(&row.rec_id));
        let page = Arc::new(ReportSyncPage {
            ticket: active.ticket,
            request_uid: wire.request_uid,
            epoch: wire.epoch,
            from_rec_id: active.current_request.from_rec_id,
            last_rec_id: wire.last_rec_id,
            max_rec_id: wire.max_rec_id,
            rows: rows.into(),
            database_recreated,
            wire_row_count: wire.row_count,
        });
        active.awaiting_apply = Some(Arc::clone(&page));
        out.push(ReportEvent::SyncPage(page));
        controls.push(ReportControl::PageReceived {
            request_uid: wire.request_uid,
        });
        true
    }

    pub(crate) fn page_applied(
        &mut self,
        page: &ReportSyncPage,
        out: &mut Vec<ReportEvent>,
    ) -> ReportPageApplyAction {
        let Some(active) = self.active.as_mut() else {
            return ReportPageApplyAction::Ignored;
        };
        let Some(awaiting) = active.awaiting_apply.as_ref() else {
            return ReportPageApplyAction::Ignored;
        };
        if awaiting.ticket != page.ticket
            || awaiting.request_uid != page.request_uid
            || awaiting.epoch != page.epoch
            || awaiting.from_rec_id != page.from_rec_id
            || awaiting.last_rec_id != page.last_rec_id
            || awaiting.max_rec_id != page.max_rec_id
            || awaiting.database_recreated != page.database_recreated
            || awaiting.wire_row_count != page.wire_row_count
            || !Arc::ptr_eq(&awaiting.rows, &page.rows)
        {
            return ReportPageApplyAction::Ignored;
        }
        let applied = active.awaiting_apply.take().expect("page checked above");
        active.page_count = active.page_count.saturating_add(1);
        active.total_rows = active
            .total_rows
            .saturating_add(u32::try_from(applied.rows.len()).unwrap_or(u32::MAX));

        if applied.database_recreated {
            let request = ReportSyncRequest::fresh(active.initial_history_depth);
            let request_uid = random_nonzero_u64();
            active.current_request = request;
            active.current_request_uid = request_uid;
            active.expected_epoch = Some(applied.epoch);
            active.current_epoch = None;
            active.live_touched.clear();
            active.deleted_overrides.clear();
            return ReportPageApplyAction::SendNext {
                request_uid,
                request,
                expected_epoch: Some(applied.epoch),
            };
        }

        if applied.is_complete() {
            let active = self.active.take().expect("active sync checked above");
            let next_from_rec_id = applied.max_rec_id.saturating_add(1);
            let durable_request = ReportSyncRequest::resume(next_from_rec_id);
            let epoch = active
                .current_epoch
                .expect("accepted report page establishes epoch");
            out.push(ReportEvent::SyncComplete(ReportSyncComplete {
                ticket: active.ticket,
                page_count: active.page_count,
                total_rows: active.total_rows,
                epoch,
                max_rec_id: applied.max_rec_id,
                next_from_rec_id,
            }));
            return ReportPageApplyAction::Complete {
                received_request_uid: applied.request_uid,
                durable_request,
                epoch,
            };
        }

        let request = ReportSyncRequest::resume(applied.last_rec_id.saturating_add(1));
        let request_uid = random_nonzero_u64();
        active.current_request = request;
        active.current_request_uid = request_uid;
        active.live_touched.clear();
        ReportPageApplyAction::SendNext {
            request_uid,
            request,
            expected_epoch: Some(applied.epoch),
        }
    }

    fn resolve_open_row_check(
        &mut self,
        rec_id: i64,
        out: &mut Vec<ReportEvent>,
        controls: &mut Vec<ReportControl>,
    ) {
        let Some(check) = self.active_check.as_mut() else {
            return;
        };
        if !check.pending.remove(&rec_id) || !check.pending.is_empty() {
            return;
        }
        let check = self
            .active_check
            .take()
            .expect("open-row check just completed");
        out.push(ReportEvent::OpenRowsCheckComplete {
            rec_ids: check.rec_ids,
        });
        controls.push(ReportControl::OpenRowsCheckCompleted);
    }
}

fn random_nonzero_u64() -> u64 {
    loop {
        let value = rand::random();
        if value != 0 {
            return value;
        }
    }
}

fn quote_sqlite_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn read_u8(data: &[u8], pos: &mut usize) -> Option<u8> {
    let value = *data.get(*pos)?;
    *pos += 1;
    Some(value)
}

fn read_u16(data: &[u8], pos: &mut usize) -> Option<u16> {
    let end = pos.checked_add(2)?;
    let value = u16::from_le_bytes(data.get(*pos..end)?.try_into().ok()?);
    *pos = end;
    Some(value)
}

fn read_i64(data: &[u8], pos: &mut usize) -> Option<i64> {
    let end = pos.checked_add(8)?;
    let value = i64::from_le_bytes(data.get(*pos..end)?.try_into().ok()?);
    *pos = end;
    Some(value)
}

fn read_f64(data: &[u8], pos: &mut usize) -> Option<f64> {
    let end = pos.checked_add(8)?;
    let value = f64::from_le_bytes(data.get(*pos..end)?.try_into().ok()?);
    *pos = end;
    Some(value)
}

fn read_str8(data: &[u8], pos: &mut usize) -> Option<String> {
    let len = usize::from(read_u8(data, pos)?);
    let end = pos.checked_add(len)?;
    let value = decode_utf8_delphi(data.get(*pos..end)?);
    *pos = end;
    Some(value)
}

impl fmt::Display for ReportHistoryDepth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ServerDefault => f.write_str("server-default"),
            Self::Days(days) => write!(f, "{days} days"),
            Self::All => f.write_str("all history"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::report::RepSyncPage as WireSyncPage;
    use crate::commands::trade::BaseCommandHeader;
    use crate::compression::synlz_compress;

    fn header(cmd_id: u8) -> BaseCommandHeader {
        BaseCommandHeader {
            cmd_id,
            ver: 3,
            uid: 1,
        }
    }

    fn schema_blob_with_status_spec(status_sql_spec: &str) -> Vec<u8> {
        let mut raw = vec![1];
        raw.extend_from_slice(&2u16.to_le_bytes());
        raw.push(6);
        raw.extend_from_slice(b"Status");
        raw.push(1);
        raw.push(u8::try_from(status_sql_spec.len()).unwrap());
        raw.extend_from_slice(status_sql_spec.as_bytes());
        raw.push(8);
        raw.extend_from_slice(b"newRecID");
        raw.push(1);
        raw.push(23);
        raw.extend_from_slice(b"sqlite3_int64 default 0");
        synlz_compress(&raw)
    }

    fn schema_blob() -> Vec<u8> {
        schema_blob_with_status_spec("INT")
    }

    fn schema_blob_with_deleted() -> Vec<u8> {
        let mut raw = vec![1];
        raw.extend_from_slice(&3u16.to_le_bytes());
        for (name, sql_spec) in [
            ("Status", "INT"),
            ("newRecID", "sqlite3_int64 default 0"),
            ("deleted", "INT default 0"),
        ] {
            raw.push(u8::try_from(name.len()).unwrap());
            raw.extend_from_slice(name.as_bytes());
            raw.push(1);
            raw.push(u8::try_from(sql_spec.len()).unwrap());
            raw.extend_from_slice(sql_spec.as_bytes());
        }
        synlz_compress(&raw)
    }

    fn row(rec_id: i64, status: i64) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&2u16.to_le_bytes());
        raw.extend_from_slice(&0u16.to_le_bytes());
        raw.push(1);
        raw.extend_from_slice(&status.to_le_bytes());
        raw.extend_from_slice(&1u16.to_le_bytes());
        raw.push(1);
        raw.extend_from_slice(&rec_id.to_le_bytes());
        raw
    }

    fn row_with_deleted(rec_id: i64, status: i64, deleted: i64) -> Vec<u8> {
        let mut raw = row(rec_id, status);
        raw[0..2].copy_from_slice(&3u16.to_le_bytes());
        raw.extend_from_slice(&2u16.to_le_bytes());
        raw.push(1);
        raw.extend_from_slice(&deleted.to_le_bytes());
        raw
    }

    fn rlelz_blob(raw: &[u8]) -> Vec<u8> {
        assert!(raw.len() < 128);
        let mut encoded = vec![raw.len() as u8, 0];
        encoded.extend_from_slice(&synlz_compress(raw));
        encoded
    }

    fn ready_state() -> ReportReplicationState {
        let mut state = ReportReplicationState::default();
        let mut out = Vec::new();
        let mut controls = Vec::new();
        assert!(state.apply_schema(
            WireSchema {
                header: header(38),
                data: schema_blob(),
            },
            &mut out,
            &mut controls,
        ));
        state
    }

    fn ready_state_with_deleted() -> ReportReplicationState {
        let mut state = ReportReplicationState::default();
        let mut out = Vec::new();
        let mut controls = Vec::new();
        assert!(state.apply_schema(
            WireSchema {
                header: header(38),
                data: schema_blob_with_deleted(),
            },
            &mut out,
            &mut controls,
        ));
        state
    }

    #[test]
    fn rows_deleted_batches_stay_near_one_kib_without_losing_selection() {
        let ranges = (0..80)
            .map(|index| ReportRecIdRange::new(index * 10, index * 10 + 9))
            .collect::<Vec<_>>();
        let singles = (1_000..1_180).collect::<Vec<_>>();
        let change = ReportRowsDeleted::new(true, ranges.iter().copied(), singles.iter().copied());

        let batches = change.wire_batches();
        assert!(batches.len() > 1);
        assert_eq!(
            batches
                .iter()
                .flat_map(|batch| batch.ranges.iter().copied())
                .collect::<Vec<_>>(),
            ranges
        );
        assert_eq!(
            batches
                .iter()
                .flat_map(|batch| batch.singles.iter().copied())
                .collect::<Vec<_>>(),
            singles
        );
        for batch in &batches {
            assert!(
                crate::commands::report::build_set_rows_deleted(1, batch).len()
                    <= crate::commands::report::MAX_SET_ROWS_DELETED_WIRE_BYTES
            );
        }
        assert!(ReportRowsDeleted::new(false, [], [])
            .wire_batches()
            .is_empty());
    }

    #[test]
    fn schema_ddl_quotes_names_and_keys_new_rec_id() {
        let state = ready_state();
        let schema = state.schema().unwrap();
        let ddl = schema.sqlite_create_table_sql("Orders");
        assert!(ddl.contains("\"Status\" INT"));
        assert!(ddl.contains("PRIMARY KEY (\"newRecID\")"));
    }

    #[test]
    fn sync_request_reserves_zero_and_max_depth_sentinels() {
        assert!(ReportSyncRequest::fresh(ReportHistoryDepth::ServerDefault).is_valid());
        assert!(ReportSyncRequest::fresh(ReportHistoryDepth::Days(1)).is_valid());
        assert!(ReportSyncRequest::fresh(ReportHistoryDepth::All).is_valid());
        assert!(!ReportSyncRequest::fresh(ReportHistoryDepth::Days(0)).is_valid());
        assert!(!ReportSyncRequest::fresh(ReportHistoryDepth::Days(u16::MAX)).is_valid());
        assert!(!ReportSyncRequest::resume(-1).is_valid());
    }

    #[test]
    fn alive_map_is_strict_and_all_live_changes_win() {
        let mut state = ready_state_with_deleted();
        let ticket = ReportAliveMapTicket { sync_id: 71 };
        let request = ReportAliveMapRequest {
            epoch: 9001,
            up_to_rec_id: 20,
        };
        let request_uid = state.begin_alive_map(ticket, request);
        let mut out = Vec::new();
        let mut controls = Vec::new();

        state.apply_rows_deleted(
            ReportRowsDeleted::new(true, [ReportRecIdRange::new(9, 16)], [2]),
            &mut out,
        );
        assert!(state.apply_live_delete(3, &mut out, &mut controls));
        assert!(state.apply_live_upsert(4, &row_with_deleted(4, 1, 1), &mut out, &mut controls,));
        assert!(state.apply_live_upsert(5, &row_with_deleted(5, 1, 0), &mut out, &mut controls,));
        out.clear();
        controls.clear();

        assert!(state.apply_alive_map(
            crate::commands::report::RepAliveMap {
                header: header(crate::commands::report::CMD_ALIVE_MAP),
                request_uid,
                epoch: 9001,
                covered_up_to: 20,
                data: rlelz_blob(&[0xff, 0xff, 0x0f]),
            },
            &mut out,
            &mut controls,
        ));
        let ReportEvent::AliveMapComplete(done) = &out[0] else {
            panic!("expected alive-map completion")
        };
        assert_eq!(done.ticket, ticket);
        assert_eq!(done.epoch, 9001);
        assert_eq!(done.is_alive(1), Some(true));
        assert_eq!(done.is_alive(2), Some(false));
        assert_eq!(done.is_alive(3), Some(false));
        assert_eq!(done.is_alive(4), Some(false));
        assert_eq!(done.is_alive(5), Some(true));
        for rec_id in 9..=16 {
            assert_eq!(done.is_alive(rec_id), Some(false));
        }
        assert_eq!(done.is_alive(21), None);
        assert!(matches!(
            controls.as_slice(),
            [ReportControl::AliveMapReceived { request_uid: uid }] if *uid == request_uid
        ));
    }

    #[test]
    fn alive_map_detects_another_database_epoch() {
        let mut state = ready_state();
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_alive_map(
            ReportAliveMapTicket { sync_id: 73 },
            ReportAliveMapRequest {
                epoch: 111,
                up_to_rec_id: 100,
            },
        );
        assert!(state.apply_alive_map(
            crate::commands::report::RepAliveMap {
                header: header(crate::commands::report::CMD_ALIVE_MAP),
                request_uid,
                epoch: 222,
                covered_up_to: 80,
                data: vec![1, 2, 3],
            },
            &mut out,
            &mut controls,
        ));
        assert!(matches!(
            out.as_slice(),
            [ReportEvent::AliveMapComplete(ReportAliveMapComplete {
                outcome: ReportAliveMapOutcome::DatabaseRecreated,
                ..
            })]
        ));
    }

    #[test]
    fn malformed_alive_map_does_not_cancel_the_retryable_request() {
        let mut state = ready_state();
        let request = ReportAliveMapRequest {
            epoch: 333,
            up_to_rec_id: 9,
        };
        let request_uid = state.begin_alive_map(ReportAliveMapTicket { sync_id: 74 }, request);
        let mut out = Vec::new();
        let mut controls = Vec::new();

        assert!(!state.apply_alive_map(
            crate::commands::report::RepAliveMap {
                header: header(crate::commands::report::CMD_ALIVE_MAP),
                request_uid,
                epoch: 333,
                covered_up_to: 9,
                data: rlelz_blob(&[0xff, 0x80]),
            },
            &mut out,
            &mut controls,
        ));
        assert!(out.is_empty());
        assert_eq!(state.retry_active_alive_map(), Some((request_uid, request)));
    }

    #[test]
    fn schema_rejects_changes_to_existing_sql_declaration() {
        let mut state = ready_state();
        let mut out = Vec::new();
        let mut controls = Vec::new();
        assert!(state.apply_schema(
            WireSchema {
                header: header(38),
                data: schema_blob_with_status_spec("INT DEFAULT 0"),
            },
            &mut out,
            &mut controls,
        ));
        assert!(matches!(
            out.as_slice(),
            [ReportEvent::SchemaRejected { .. }]
        ));
        assert_eq!(state.schema().unwrap().revision(), 2);
    }

    #[test]
    fn live_upsert_before_schema_waits_for_the_deferred_catch_up() {
        let mut state = ReportReplicationState::default();
        let mut out = Vec::new();
        let mut controls = Vec::new();
        assert!(state.apply_live_upsert(7, &row(99, 1), &mut out, &mut controls));
        assert!(out.is_empty());
        assert!(state.apply_live_delete(7, &mut out, &mut controls));
        assert!(out.is_empty());
    }

    #[test]
    fn page_is_published_once_and_next_request_waits_for_application_ack() {
        let mut state = ready_state();
        let ticket = ReportSyncTicket { sync_id: 55 };
        let request = ReportSyncRequest::fresh(ReportHistoryDepth::ServerDefault);
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, request, None, &mut out);
        out.clear();

        let page = WireSyncPage {
            header: header(39),
            request_uid,
            epoch: 123,
            last_rec_id: 7,
            max_rec_id: 9,
            row_count: 1,
            blob: synlz_compress(&row(7, 1)),
        };
        assert!(state.apply_sync_page(page.clone(), &mut out, &mut controls,));
        let ReportEvent::SyncPage(page_event) = &out[0] else {
            panic!("expected page")
        };
        let page_event = Arc::clone(page_event);
        assert_eq!(page_event.rows.len(), 1);
        assert!(matches!(
            controls.as_slice(),
            [ReportControl::PageReceived { .. }]
        ));

        out.clear();
        controls.clear();
        assert!(state.apply_sync_page(page, &mut out, &mut controls,));
        assert!(out.is_empty(), "duplicate page must not be published twice");

        let action = state.page_applied(&page_event, &mut out);
        let ReportPageApplyAction::SendNext { request, .. } = action else {
            panic!("expected next page request")
        };
        assert_eq!(request, ReportSyncRequest::resume(8));
        assert!(out.is_empty());
    }

    #[test]
    fn final_page_completes_only_after_application_ack() {
        let mut state = ready_state();
        let ticket = ReportSyncTicket { sync_id: 41 };
        let request = ReportSyncRequest::resume(7);
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, request, None, &mut out);
        out.clear();

        assert!(state.apply_sync_page(
            WireSyncPage {
                header: header(39),
                request_uid,
                epoch: 123,
                last_rec_id: 7,
                max_rec_id: 7,
                row_count: 1,
                blob: synlz_compress(&row(7, 1)),
            },
            &mut out,
            &mut controls,
        ));
        let ReportEvent::SyncPage(page) = out.pop().unwrap() else {
            panic!("expected page")
        };
        assert!(page.is_complete());
        assert!(out.is_empty());

        let action = state.page_applied(&page, &mut out);
        let ReportPageApplyAction::Complete {
            durable_request, ..
        } = action
        else {
            panic!("expected completion")
        };
        assert_eq!(durable_request, ReportSyncRequest::resume(8));
        let ReportEvent::SyncComplete(done) = &out[0] else {
            panic!("expected completion event")
        };
        assert_eq!(done.ticket, ticket);
        assert_eq!(done.page_count, 1);
        assert_eq!(done.total_rows, 1);
        assert_eq!(done.epoch, 123);
        assert_eq!(done.next_from_rec_id, 8);
        assert_eq!(
            done.checkpoint(),
            ReportSyncCheckpoint {
                epoch: 123,
                next_from_rec_id: 8,
            }
        );
    }

    #[test]
    fn live_update_wins_over_an_older_copy_in_the_current_page() {
        let mut state = ready_state();
        let ticket = ReportSyncTicket { sync_id: 45 };
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, ReportSyncRequest::resume(5), None, &mut out);
        out.clear();

        assert!(state.apply_live_upsert(7, &row(7, 1), &mut out, &mut controls));
        out.clear();
        controls.clear();
        let mut raw = row(7, 0);
        raw.extend_from_slice(&row(8, 0));
        assert!(state.apply_sync_page(
            WireSyncPage {
                header: header(39),
                request_uid,
                epoch: 123,
                last_rec_id: 8,
                max_rec_id: 9,
                row_count: 2,
                blob: synlz_compress(&raw),
            },
            &mut out,
            &mut controls,
        ));
        let ReportEvent::SyncPage(page) = &out[0] else {
            panic!("expected page")
        };
        assert_eq!(page.source_row_count(), 2);
        assert_eq!(
            page.rows.iter().map(|row| row.rec_id).collect::<Vec<_>>(),
            [8]
        );
        assert!(!page.is_complete());
    }

    #[test]
    fn rows_deleted_echo_wins_over_an_older_copy_in_the_current_sync() {
        let mut state = ready_state_with_deleted();
        let schema = state.schema().unwrap().clone();
        let ticket = ReportSyncTicket { sync_id: 46 };
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, ReportSyncRequest::resume(5), None, &mut out);
        out.clear();

        let change = ReportRowsDeleted::new(true, [], [7]);
        state.apply_rows_deleted(change.clone(), &mut out);
        assert_eq!(out, [ReportEvent::RowsDeleted(change)]);
        out.clear();

        assert!(state.apply_sync_page(
            WireSyncPage {
                header: header(39),
                request_uid,
                epoch: 123,
                last_rec_id: 7,
                max_rec_id: 8,
                row_count: 1,
                blob: synlz_compress(&row_with_deleted(7, 1, 0)),
            },
            &mut out,
            &mut controls,
        ));
        let ReportEvent::SyncPage(page) = &out[0] else {
            panic!("expected page")
        };
        assert_eq!(page.rows[0].integer_by_name(&schema, "deleted"), Some(1));
    }

    #[test]
    fn wrong_request_uid_is_ignored() {
        let mut state = ready_state();
        let ticket = ReportSyncTicket { sync_id: 51 };
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, ReportSyncRequest::resume(5), None, &mut out);
        out.clear();

        assert!(state.apply_sync_page(
            WireSyncPage {
                header: header(39),
                request_uid: request_uid.wrapping_add(1),
                epoch: 123,
                last_rec_id: 7,
                max_rec_id: 7,
                row_count: 1,
                blob: synlz_compress(&row(7, 1)),
            },
            &mut out,
            &mut controls,
        ));
        assert!(out.is_empty());
    }

    #[test]
    fn retry_keeps_request_uid_so_a_delayed_page_remains_valid() {
        let mut state = ready_state();
        let ticket = ReportSyncTicket { sync_id: 52 };
        let request = ReportSyncRequest::resume(5);
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, request, None, &mut out);
        out.clear();

        let (retry_uid, retry_request) = state.retry_active_page().unwrap();
        assert_eq!(retry_uid, request_uid);
        assert_eq!(retry_request, request);

        assert!(state.apply_sync_page(
            WireSyncPage {
                header: header(39),
                request_uid,
                epoch: 123,
                last_rec_id: 7,
                max_rec_id: 7,
                row_count: 1,
                blob: synlz_compress(&row(7, 1)),
            },
            &mut out,
            &mut controls,
        ));
        assert!(matches!(out.as_slice(), [ReportEvent::SyncPage(_)]));
    }

    #[test]
    fn database_recreation_waits_for_clear_ack_then_restarts_from_zero() {
        let mut state = ready_state();
        let ticket = ReportSyncTicket { sync_id: 61 };
        let request = ReportSyncRequest::resume(100);
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, request, None, &mut out);
        out.clear();

        assert!(state.apply_sync_page(
            WireSyncPage {
                header: header(39),
                request_uid,
                epoch: 123,
                last_rec_id: 0,
                max_rec_id: 50,
                row_count: 0,
                blob: Vec::new(),
            },
            &mut out,
            &mut controls,
        ));
        let ReportEvent::SyncPage(page) = out.pop().unwrap() else {
            panic!("expected recreate page")
        };
        assert!(page.database_recreated);
        let ReportPageApplyAction::SendNext { request, .. } = state.page_applied(&page, &mut out)
        else {
            panic!("expected fresh restart")
        };
        assert_eq!(
            request,
            ReportSyncRequest::fresh(ReportHistoryDepth::ServerDefault)
        );
        assert!(out.is_empty());
    }

    #[test]
    fn persisted_epoch_detects_recreated_database_even_when_highwater_grew() {
        let mut state = ready_state();
        let ticket = ReportSyncTicket { sync_id: 62 };
        let request = ReportSyncRequest::resume(100);
        let mut out = Vec::new();
        let mut controls = Vec::new();
        let request_uid = state.begin_sync(ticket, request, Some(777), &mut out);
        out.clear();

        assert!(state.apply_sync_page(
            WireSyncPage {
                header: header(39),
                request_uid,
                epoch: 888,
                last_rec_id: 100,
                max_rec_id: 500,
                row_count: 1,
                blob: synlz_compress(&row(100, 1)),
            },
            &mut out,
            &mut controls,
        ));
        let ReportEvent::SyncPage(page) = out.pop().unwrap() else {
            panic!("expected recreate page")
        };
        assert!(page.database_recreated);
        assert!(page.rows.is_empty());
        let ReportPageApplyAction::SendNext {
            request,
            expected_epoch,
            ..
        } = state.page_applied(&page, &mut out)
        else {
            panic!("expected fresh restart")
        };
        assert_eq!(
            request,
            ReportSyncRequest::fresh(ReportHistoryDepth::ServerDefault)
        );
        assert_eq!(expected_epoch, Some(888));
    }

    #[test]
    fn open_rows_check_completes_after_one_authoritative_result_per_id() {
        let mut state = ready_state();
        let ids: Arc<[i64]> = vec![7, 8].into();
        let mut out = Vec::new();
        let mut controls = Vec::new();
        state.begin_open_rows_check(Arc::clone(&ids), &mut out);
        out.clear();

        assert!(state.apply_live_upsert(7, &row(7, 1), &mut out, &mut controls));
        assert!(matches!(out.as_slice(), [ReportEvent::RowUpsert(_)]));
        out.clear();
        assert!(state.apply_live_delete(8, &mut out, &mut controls));
        assert!(matches!(
            out.as_slice(),
            [
                ReportEvent::RowDelete { rec_id: 8 },
                ReportEvent::OpenRowsCheckComplete { .. }
            ]
        ));
        assert!(matches!(
            controls.as_slice(),
            [ReportControl::OpenRowsCheckCompleted]
        ));
    }

    #[test]
    fn row_rejects_unknown_value_kind() {
        let mut malformed = Vec::new();
        malformed.extend_from_slice(&1u16.to_le_bytes());
        malformed.extend_from_slice(&0u16.to_le_bytes());
        malformed.push(99);
        assert!(ReportRow::parse(&malformed, 1, Some(7)).is_none());
    }
}
