use async_trait::async_trait;
use log::info;

use super::PodState;
use kubelet::state::prelude::*;

/// The Kubelet is aware of the Pod.
#[derive(Default, Debug)]
pub struct Registered;

#[async_trait]
impl State<PodState> for Registered {
    async fn next(
        self: Box<Self>,
        _pod_state: &mut PodState,
        pod: &Pod,
    ) -> anyhow::Result<Transition<PodState>> {
        info!("Pod added: {}.", pod.name());
        unimplemented!()
    }

    async fn json_status(
        &self,
        _pod_state: &mut PodState,
        _pod: &Pod,
    ) -> anyhow::Result<serde_json::Value> {
        make_status(Phase::Pending, "Registered")
    }
}
