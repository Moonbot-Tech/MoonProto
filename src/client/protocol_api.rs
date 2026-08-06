use super::*;

impl Client {
    pub(crate) fn engine_response_request_uid_from_payload(payload: &[u8]) -> Option<u64> {
        // Engine response payload includes 11-byte TBaseCommand header, then
        // RequestUID. This is enough to cheaply check ApiPending without
        // inflating a full response in the receive phase.
        let uid = payload.get(11..19)?;
        Some(u64::from_le_bytes(uid.try_into().unwrap()))
    }

    pub(crate) fn engine_response_meta_from_payload(payload: &[u8]) -> Option<EngineResponseMeta> {
        if payload.len() < 11 {
            return None;
        }
        let mut pos = 11usize;
        let request_uid = u64::from_le_bytes(payload.get(pos..pos + 8)?.try_into().ok()?);
        pos += 8;
        let method = EngineMethod::from_byte(*payload.get(pos)?);
        pos += 1;
        let success = *payload.get(pos)? != 0;
        pos += 1;
        // ErrorCode.
        payload.get(pos..pos + 4)?;
        pos += 4;
        // ErrorMsg string, length-prefixed UTF-8. Skip only; no allocation.
        let len = u16::from_le_bytes(payload.get(pos..pos + 2)?.try_into().ok()?) as usize;
        pos += 2;
        payload.get(pos..pos + len)?;
        Some(EngineResponseMeta {
            request_uid,
            method,
            success,
        })
    }

    pub(crate) fn engine_response_method_from_payload(payload: &[u8]) -> Option<EngineMethod> {
        payload.get(19).copied().map(EngineMethod::from_byte)
    }

    pub(crate) fn apply_engine_response_client_bookkeeping(&mut self, resp: &EngineResponse) {
        // Active library: auto-clear indexes_fetch_in_flight on a
        // GetMarketsIndexes response (any one — even unsuccessful, so it never
        // hangs forever).
        if resp.method == EngineMethod::GetMarketsIndexes {
            self.reconnect.indexes_fetch_in_flight = false;
            let indexes_payload_ok = resp.success
                && crate::commands::market::parse_markets_indexes_response(&resp.data).is_some();
            if indexes_payload_ok {
                // Remember that indexes have been received for the current PeerAppToken.
                self.reconnect.tracked_indexes_peer_app_token = self.peer_app_token;
                if self.reconnect.update_markets_after_indexes {
                    self.reconnect.update_markets_after_indexes = false;
                    self.send_api_request(&crate::commands::engine_request::update_markets_list());
                }
                if self.reconnect.restore_orderbooks_after_indexes {
                    self.reconnect.restore_orderbooks_after_indexes = false;
                    self.restore_orderbook_subscriptions_from_registry();
                }
            }
        }

        // Delphi `DoSubscribeOrderBooks`: only a successful response confirms
        // the current `ServerToken`. For the reconnect batch this is a full
        // `BookSubbed` replay; an ordinary point subscription may set the token
        // only from the initial state, like Delphi `FSubscribedBookServerToken = 0`.
        if resp.method == EngineMethod::SubscribeOrderBook {
            let is_reconnect_batch =
                self.reconnect.pending_orderbook_resubscribe_uid == Some(resp.request_uid);
            if resp.success
                && (self.reconnect.subscribed_book_server_token == 0 || is_reconnect_batch)
            {
                self.reconnect.subscribed_book_server_token = self.server_token;
            }
            self.close_orderbook_subscribe_wait_if_matches(resp.request_uid);
            if is_reconnect_batch {
                self.reconnect.pending_orderbook_resubscribe_uid = None;
            }
        }

        if resp.method == EngineMethod::SubscribeCandles {
            let completed = {
                self.reconnect
                    .pending_candle_subscribes
                    .lock()
                    .finish(resp.request_uid, resp.success)
            };
            if let Some(all_succeeded) = completed {
                self.reconnect.subscribed_candle_server_token =
                    if all_succeeded { self.server_token } else { 0 };
                self.reconnect
                    .last_candle_subscribe_request_ms
                    .store(NEVER_TIME_MS, Ordering::Relaxed);
            }
        }

        // Delphi `TMoonProtoEngine.SubscribeAllTrades`: successful
        // `emk_SubscribeAllTrades` refreshes `LastReconnectCheck`.
        // Until the first TradesStream packet updates `FTradesServerToken`,
        // this 5s gate prevents immediate unsubscribe/resubscribe churn.
        if resp.method == EngineMethod::SubscribeAllTrades && resp.success {
            let now_ms = self.now_ms();
            self.reconnect.last_trades_reconnect_check_ms = now_ms;
        }
        if resp.method == EngineMethod::SubscribeAllTrades {
            self.reconnect
                .last_trades_subscribe_request_ms
                .store(NEVER_TIME_MS, Ordering::Relaxed);
        }
        if resp.method == EngineMethod::UnsubscribeAllTrades {
            self.close_trades_unsubscribe_wait_if_matches(resp.request_uid);
        }
    }

    pub(crate) fn dispatch_api_pending(api_pending: &ApiPending, cmd: u8, payload: &[u8]) -> bool {
        if cmd != Command::API.to_byte() {
            return false;
        }
        let Some(uid) = Self::engine_response_request_uid_from_payload(payload) else {
            return false;
        };
        api_pending.dispatch_registered_with(uid, || parse_engine_response(payload))
    }

    pub(crate) fn dispatch_chunked_api_response(
        pending_api: &mut PendingApi,
        cmd: u8,
        payload: &[u8],
        now_ms: i64,
    ) -> bool {
        if cmd != Command::API.to_byte() {
            return false;
        }
        let Some(method) = Self::engine_response_method_from_payload(payload) else {
            return false;
        };
        let Some(uid) = Self::engine_response_request_uid_from_payload(payload) else {
            return false;
        };
        let registered = match method {
            EngineMethod::RequestCandlesData => pending_api.pending_candles.contains_key(&uid),
            EngineMethod::RequestMarketHistory => {
                pending_api.pending_market_history.contains_key(&uid)
            }
            _ => false,
        };
        if !registered {
            return false;
        }
        let Some(resp) = parse_engine_response(payload) else {
            return false;
        };
        match method {
            EngineMethod::RequestCandlesData => {
                Self::handle_candles_chunk_in_pending(pending_api, &resp, now_ms)
            }
            EngineMethod::RequestMarketHistory => {
                Self::handle_market_history_chunk_in_pending(pending_api, &resp)
            }
            _ => false,
        }
    }

    pub(crate) fn client_new_data_decoded(
        &mut self,
        cmd: u8,
        payload: Vec<u8>,
        api_pending_consumed_by_reader: bool,
        chunked_response_consumed_by_reader: bool,
        payload_buf: &mut Vec<(Command, Vec<u8>)>,
    ) {
        if cmd == Command::API.to_byte() {
            match self.process_api_command_decoded(
                payload,
                api_pending_consumed_by_reader,
                chunked_response_consumed_by_reader,
                payload_buf,
            ) {
                Ok(()) => {
                    return;
                }
                Err(payload) => {
                    payload_buf.push((Command::from_byte(cmd), payload));
                    return;
                }
            }
        }

        payload_buf.push((Command::from_byte(cmd), payload));
    }

    pub(crate) fn process_api_command_decoded(
        &mut self,
        payload: Vec<u8>,
        api_pending_consumed_by_reader: bool,
        chunked_response_consumed_by_reader: bool,
        payload_buf: &mut Vec<(Command, Vec<u8>)>,
    ) -> Result<(), Vec<u8>> {
        // Engine API responses first enter their registered pending collector.
        // Unregistered responses remain available to the ordinary data path.
        if chunked_response_consumed_by_reader {
            return Ok(());
        }
        if let Some(meta) = Self::engine_response_meta_from_payload(&payload) {
            // Chunked responses keep their pending slot until every packet with
            // the same UID has been assembled.
            let now_ms = self.now_ms();
            if meta.method == EngineMethod::RequestCandlesData {
                if let Some(resp) = parse_engine_response(&payload) {
                    if Self::handle_candles_chunk_in_pending(&mut self.pending_api, &resp, now_ms) {
                        // Async consumers get the merged result via
                        // Receiver<MergedCandles>. The active dispatcher only
                        // sees completed/ordinary API payloads.
                        return Ok(());
                    }
                }
            }
            if meta.method == EngineMethod::RequestMarketHistory {
                if let Some(resp) = parse_engine_response(&payload) {
                    if Self::handle_market_history_chunk_in_pending(&mut self.pending_api, &resp) {
                        return Ok(());
                    }
                }
            }
            // An unregistered response falls back to the ordinary pending/event
            // path used by fire-and-forget API callers.

            let pending_side_effect_owner =
                api_pending_consumed_by_reader && method_applies_after_pending(meta.method);
            if pending_side_effect_owner {
                // Delphi `ProcessApiCommand` only stores the response into
                // `PendingRequests`; `TMoonProtoEngine.GetMarketsList` /
                // `UpdateMarketsList` applies heavy market state after
                // `SendAndWait` returns. Keep Rust's protocol dispatch path
                // equally thin: the runtime/init owner applies these payloads
                // from the pending receiver instead of doing a second parse here.
                return Ok(());
            }

            let Some(resp) = parse_engine_response(&payload) else {
                return Err(payload);
            };

            self.apply_engine_response_client_bookkeeping(&resp);

            // 2. Pending registry (ordinary async API).
            if !api_pending_consumed_by_reader {
                let _ = self.pending_api.api_pending.dispatch(resp);
            }
            // Active state must update regardless of whether user code also
            // awaited this response via a Receiver.
            payload_buf.push((Command::API, payload));
            return Ok(());
        }
        // Failed to parse — fall back to the raw sink.
        Err(payload)
    }

    /// Absorb a candles chunk through the pending aggregator. Returns `true` if the
    /// slot was found and the chunk was processed (even if merged is not ready yet;
    /// keep accumulating); `false` if the UID is not registered (the consumer does
    /// not use the async API).
    ///
    /// When the aggregator returns merged, the completed `MergedCandles` is sent to
    /// the sender and the slot is removed. If the sender has already been dropped
    /// (no receiver waiting), the slot is removed anyway (semantics =
    /// "fire-and-forget with finalization").
    pub(crate) fn handle_candles_chunk_in_pending(
        pending_api: &mut PendingApi,
        resp: &EngineResponse,
        _now_ms: i64,
    ) -> bool {
        // Keep the slot until the response is complete or explicitly fails.
        if !resp.success {
            if let Some(partial) = pending_api.pending_candles.remove(&resp.request_uid) {
                log::warn!(target: "moonproto::client",
                    "candles request uid={} failed code={} msg={}",
                    resp.request_uid, resp.error_code, resp.error_msg);
                drop(partial);
                return true;
            }
            return false;
        }

        let uid = resp.request_uid;
        let chunk_result = {
            let Some(partial) = pending_api.pending_candles.get_mut(&uid) else {
                return false;
            };
            let chunk_result = partial.aggregator.on_chunk_result(&resp.data);
            if matches!(
                chunk_result,
                CandlesChunkResult::Stored | CandlesChunkResult::Complete(_)
            ) {
                partial.progress.mark_stored();
            }
            chunk_result
        };
        if let CandlesChunkResult::Complete(zipped_data) = chunk_result {
            if let Some(partial) = pending_api.pending_candles.remove(&uid) {
                pending_api
                    .chunked_parse
                    .submit_candles(uid, zipped_data, partial.sender);
            }
        }
        true
    }

    pub(crate) fn handle_market_history_chunk_in_pending(
        pending_api: &mut PendingApi,
        resp: &EngineResponse,
    ) -> bool {
        let uid = resp.request_uid;
        if !resp.success {
            if let Some(partial) = pending_api.pending_market_history.remove(&uid) {
                let _ = partial.sender.send(Err(format!(
                    "RequestMarketHistory failed with code {}: {}",
                    resp.error_code, resp.error_msg
                )));
                return true;
            }
            return false;
        }

        let chunk_result = {
            let Some(partial) = pending_api.pending_market_history.get_mut(&uid) else {
                return false;
            };
            let result = partial.aggregator.on_chunk(&resp.data);
            if matches!(
                result,
                crate::commands::chunked_response::ChunkedResponseResult::Stored
                    | crate::commands::chunked_response::ChunkedResponseResult::Complete(_)
            ) {
                partial.progress.mark_stored();
            }
            result
        };
        if let crate::commands::chunked_response::ChunkedResponseResult::Complete(compressed) =
            chunk_result
        {
            if let Some(partial) = pending_api.pending_market_history.remove(&uid) {
                pending_api
                    .chunked_parse
                    .submit_market_history(uid, compressed, partial.sender);
            }
        }
        true
    }
}

#[inline]
fn method_applies_after_pending(method: EngineMethod) -> bool {
    matches!(
        method,
        EngineMethod::GetMarketsList | EngineMethod::UpdateMarketsList
    )
}
