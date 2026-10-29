use super::*;
use crate::bench_support::measure;

#[test]
#[ignore = "CPU/allocation benchmark; release, --nocapture --test-threads=1"]
fn followup_snapshot_benchmark() {
    let client = Client::new(super::tests::dummy_cfg());
    let mut dispatcher = crate::events::EventDispatcher::new();
    let snapshot = RwLock::new(None);
    for (changed, events) in [(true, true), (true, false), (false, true), (false, false)] {
        measure(&format!("snapshot_{changed}_{events}"), 8192, || {
            if events {
                dispatcher.queue_events([crate::Event::Order(crate::state::OrderEvent::Snapshot)]);
            }
            let ready = take_queued_events_and_publish_snapshot(&client, &mut dispatcher, &snapshot, changed);
            assert_eq!(ready.len(), usize::from(events));
            std::hint::black_box(ready);
        });
    }
}
