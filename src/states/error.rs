use kubelet::state::prelude::*;

use super::PodState;

#[derive(Default, Debug)]
/// The Pod failed to run.
pub struct Error {
    pub message: String,
}

#[async_trait::async_trait]
impl State<PodState> for Error {
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
        make_status(Phase::Pending, &self.message)
    }
}
