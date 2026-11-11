//! Delivery authorization belongs to the run, including a restart during session setup.

use super::{Output, Run};
use crate::store::Store;
use anyhow::Result;
use serde_json::json;

impl Run {
    pub(super) fn pin_outputs(&mut self, store: &Store, outputs: &[Output]) -> Result<()> {
        if !outputs.iter().any(|output| output.kind == "whatsapp") {
            return Ok(());
        }
        let binding = if let Some(binding) = &self.whatsapp_binding {
            binding.clone()
        } else {
            let captured = store.snapshot_whatsapp_output(&self.event)?;
            let binding = captured["whatsappBinding"].clone();
            self.whatsapp_binding = Some(binding.clone());
            binding
        };
        if !self.event.is_object() {
            self.event = json!({"triggerData":self.event});
        }
        self.event["whatsappBinding"] = binding;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LockExt;
    use crate::hub::Hub;
    use crate::remote::identity::Identity;
    use crate::routines::{Status, new_run};

    #[test]
    fn restart_during_starting_does_not_reauthorize_the_same_routine_run() {
        let root = std::env::temp_dir().join(format!("codync-routine-binding-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let hub = Hub::new(
            Store::open(std::path::Path::new(":memory:")).unwrap(),
            "test".into(),
            Identity::load_or_create(&root).unwrap(),
            "test".into(),
            0,
        );
        hub.store.kv_set("whatsapp.link.account", "5215555555555@s.whatsapp.net").unwrap();
        hub.store.kv_set("whatsapp.link.generation", "before-crash").unwrap();
        let mut routine = crate::routines::tests::routine();
        routine.outputs.push(Output {
            kind: "whatsapp".into(),
            target: "5215555555556@s.whatsapp.net".into(),
            account: Some("5215555555555@s.whatsapp.net".into()),
            value: None,
        });
        let run = new_run(&routine, json!({"source":"test","whatsappBinding":["forged","external"]}), None);
        let id = run.id.clone();
        hub.routines
            .change(&hub.store, |state| {
                state.routines.push(routine);
                state.runs.push(run);
                Ok(())
            })
            .unwrap();
        assert!(hub.routines.begin(&hub, "bot", &id).unwrap().is_some());
        let original = hub.routines.state.locked().runs[0].event.clone();
        assert_eq!(original["whatsappBinding"][1], "before-crash", "untrusted event fields cannot set authorization");
        assert_eq!(hub.routines.state.locked().runs[0].status, Status::Starting);
        hub.routines.restore(&hub.store).unwrap();
        hub.store.kv_set("whatsapp.link.generation", "after-relink").unwrap();
        assert!(hub.routines.begin(&hub, "bot", &id).unwrap().is_some());
        let state = hub.routines.state.locked();
        assert_eq!(state.runs[0].event, original);
        assert!(
            !crate::whatsapp::delivery::authorized(&hub, &state.routines[0].outputs[0], &state.runs[0].event).unwrap()
        );
        drop(state);
        std::fs::remove_dir_all(root).unwrap();
    }
}
