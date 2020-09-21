use async_trait::async_trait;

mod registered;
mod terminated;

pub(crate) use registered::Registered;
pub(crate) use terminated::Terminated;
pub struct PodState;

#[async_trait]
impl kubelet::state::AsyncDrop for PodState {
    async fn async_drop(self) {}
}
