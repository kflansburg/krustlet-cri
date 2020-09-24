use super::PodState;
use async_trait::async_trait;
use k8s_cri::v1alpha2 as cri;
use kubelet::state::prelude::*;
use log::{debug, error, info, warn};

/// Pod was deleted.
#[derive(Default, Debug)]
pub struct Terminated;

pub async fn stop_and_delete_pod_sandbox(
    pod_state: &PodState,
    pod: kubelet::pod::Pod,
) -> anyhow::Result<()> {
    match pod_state
        .shared
        .pods
        .read()
        .await
        .get(&(pod.namespace().to_string(), pod.name().to_string()))
    {
        Some(pod_sandbox) => {
            debug!("Stopping pod sandbox {}", pod.name());
            let mut client = match pod_state.shared.client().await {
                Ok(client) => client,
                Err(e) => {
                    error!("Error creating client: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            let request = tonic::Request::new(cri::StopPodSandboxRequest {
                pod_sandbox_id: pod_sandbox.id.clone(),
            });
            debug!("Sending request: {:?}", &request);
            let response = match client.stop_pod_sandbox(request).await {
                Ok(response) => response,
                Err(e) => {
                    error!("Error making request: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            info!("Stopped pod sandbox {}: {:?}", pod.name(), response);

            debug!("Removing pod sandbox {}", pod.name());
            let request = tonic::Request::new(cri::RemovePodSandboxRequest {
                pod_sandbox_id: pod_sandbox.id.clone(),
            });
            debug!("Sending request: {:?}", &request);
            let response = match client.remove_pod_sandbox(request).await {
                Ok(response) => response,
                Err(e) => {
                    error!("Error making request: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            info!("Removed pod sandbox {}: {:?}", pod.name(), response);
            Ok(())
        }
        None => {
            warn!("Unkown pod.");
            Ok(())
        }
    }
}

#[async_trait]
impl State<PodState> for Terminated {
    async fn next(
        self: Box<Self>,
        pod_state: &mut PodState,
        pod: &Pod,
    ) -> anyhow::Result<Transition<PodState>> {
        pod_state.shared.refresh_pods().await?;
        stop_and_delete_pod_sandbox(&pod_state, pod.clone()).await?;
        let dp = kube::api::DeleteParams {
            grace_period_seconds: Some(0),
            ..Default::default()
        };
        let pod_client: kube::Api<k8s_openapi::api::core::v1::Pod> = kube::Api::namespaced(
            kube::client::Client::new(pod_state.shared.kubeconfig.clone()),
            pod.namespace(),
        );
        // TODO: Handle Either
        pod_client.delete(pod.name(), &dp).await?;
        Ok(Transition::Complete(Ok(())))
    }

    async fn json_status(
        &self,
        _pod_state: &mut PodState,
        _pod: &Pod,
    ) -> anyhow::Result<serde_json::Value> {
        make_status(Phase::Succeeded, "Terminated")
    }
}
