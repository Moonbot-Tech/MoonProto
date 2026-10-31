//! Complete initial core states. Records reuse the ordinary command decoders.

use crate::commands::strat::{StratCommand, StratRuntimeState};
use crate::commands::ui::{
    HyperliquidRequestLimitStateCommand, KernelLicenseStateCommand, ProfitStateCommand, RuntimeStateCommand, UICommand,
};

pub(crate) struct StartupSnapshot {
    pub(crate) runtime: RuntimeStateCommand,
    pub(crate) license: KernelLicenseStateCommand,
    pub(crate) profit: ProfitStateCommand,
    pub(crate) hyperliquid: HyperliquidRequestLimitStateCommand,
    pub(crate) strategies: StratRuntimeState,
}

pub(crate) fn parse_startup_state(data: &[u8]) -> Option<StartupSnapshot> {
    if data.first() != Some(&1) {
        return None;
    }
    let mut rest = &data[1..];
    // Legacy standalone commands allow absent tails. The new complete snapshot
    // must contain every field of its first version before any state is applied.
    let UICommand::RuntimeState(runtime) = UICommand::parse(take_record(&mut rest, 13)?)? else {
        return None;
    };
    let UICommand::KernelLicenseState(license) = UICommand::parse(take_record(&mut rest, 58)?)? else {
        return None;
    };
    let UICommand::ProfitState(profit) = UICommand::parse(take_record(&mut rest, 35)?)? else {
        return None;
    };
    let UICommand::HyperliquidRequestLimitState(hyperliquid) = UICommand::parse(take_record(&mut rest, 19)?)? else {
        return None;
    };
    let StratCommand::RuntimeState(strategies) = StratCommand::parse(take_record(&mut rest, 12)?)? else {
        return None;
    };
    Some(StartupSnapshot {
        runtime,
        license,
        profit,
        hyperliquid,
        strategies,
    })
}

fn take_record<'a>(rest: &mut &'a [u8], minimum: usize) -> Option<&'a [u8]> {
    let len = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    let body = rest.get(4..)?;
    if len < minimum || len > body.len() {
        return None;
    }
    let (record, tail) = body.split_at(len);
    *rest = tail;
    Some(record)
}

#[cfg(test)]
pub(crate) fn startup_state_fixture() -> Vec<u8> {
    let mut data = vec![1];
    let mut profit = Vec::new();
    profit.extend_from_slice(&123.5_f64.to_le_bytes());
    profit.extend_from_slice(&7_i32.to_le_bytes());
    profit.extend_from_slice(&45.0_f64.to_le_bytes());
    profit.extend_from_slice(&2_i32.to_le_bytes());
    for (id, body) in [
        (20, vec![1, 1]),
        (22, vec![0; 47]),
        (24, profit),
        (30, 321_i64.to_le_bytes().to_vec()),
        (10, vec![1]),
    ] {
        data.extend_from_slice(&((11 + body.len()) as u32).to_le_bytes());
        data.push(id);
        data.extend_from_slice(&4_u16.to_le_bytes());
        data.extend_from_slice(&(100 + u64::from(id)).to_le_bytes());
        data.extend(body);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_snapshot_and_append_only_tails() {
        let mut data = startup_state_fixture();
        let state = parse_startup_state(&data).unwrap();
        assert!(state.runtime.is_started && state.runtime.auto_detect_active);
        assert_eq!(state.runtime.prepared_release_version, None, "L5 snapshot remains valid");
        assert_eq!(state.profit.rep_total_profit, 123.5);
        assert_eq!(state.profit.rep_total_trades, 7);
        assert_eq!(state.hyperliquid.requests_left, Some(321));
        assert!(state.strategies.strategies_running);
        data.extend_from_slice(b"future extension");
        assert!(parse_startup_state(&data).is_some());
    }

    #[test]
    fn prepared_release_is_recovered_with_the_initial_runtime_state() {
        let mut data = startup_state_fixture();
        data[1..5].copy_from_slice(&17u32.to_le_bytes());
        data.splice(18..18, 773u32.to_le_bytes());
        let state = parse_startup_state(&data).unwrap();
        assert_eq!(state.runtime.prepared_release_version, Some(773));
        assert_eq!(state.profit.rep_total_profit, 123.5);
        let mut dispatcher = crate::events::EventDispatcher::new();
        dispatcher.apply_startup_state(state);
        assert_eq!(dispatcher.settings().runtime_state.unwrap().prepared_release_version, Some(773));
    }

    #[test]
    fn every_truncated_prefix_is_rejected_instead_of_becoming_zero_state() {
        let data = startup_state_fixture();
        for len in 0..data.len() {
            assert!(
                parse_startup_state(&data[..len]).is_none(),
                "accepted {len} of {} bytes",
                data.len()
            );
        }
    }

    #[test]
    fn malformed_records_or_unknown_versions_cannot_open_ready() {
        let original = startup_state_fixture();
        for (offset, value) in [(0, 2), (5, 24), (6, 255), (1, 255), (1, 11)] {
            let mut data = original.clone();
            data[offset] = value;
            assert!(parse_startup_state(&data).is_none(), "offset={offset} value={value}");
        }
    }
}
