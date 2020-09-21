use async_trait::async_trait;

use super::PodState;
use kubelet::state::prelude::*;

/// The Kubelet is aware of the Pod.
#[derive(Default, Debug)]
pub struct Terminated;

#[async_trait]
impl State<PodState> for Terminated {
    async fn next(
        self: Box<Self>,
        _pod_state: &mut PodState,
        _pod: &Pod,
    ) -> anyhow::Result<Transition<PodState>> {
        unimplemented!()
    }

    async fn json_status(
        &self,
        _pod_state: &mut PodState,
        _pod: &Pod,
    ) -> anyhow::Result<serde_json::Value> {
        make_status(Phase::Succeeded, "Terminated")
    }
}
