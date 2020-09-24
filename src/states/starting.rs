use async_trait::async_trait;
use k8s_cri::v1alpha2 as cri;
use log::{debug, error, info, warn};

use super::terminated::stop_and_delete_pod_sandbox;
use super::{running::Running, PodState};
use kubelet::state::prelude::*;

/// The Kubelet is starting the Pod.
#[derive(Default, Debug)]
pub struct Starting;

#[async_trait]
impl State<PodState> for Starting {
    async fn next(
        self: Box<Self>,
        pod_state: &mut PodState,
        pod: &Pod,
    ) -> anyhow::Result<Transition<PodState>> {
        pod_state.shared.refresh_pods().await?;

        let pod_exists = {
            pod_state
                .shared
                .pods
                .read()
                .await
                .contains_key(&(pod.namespace().to_string(), pod.name().to_string()))
        };

        if pod_exists {
            stop_and_delete_pod_sandbox(&pod_state, pod.clone()).await?;
        }

        debug!("Starting pod sandbox {}", pod.name());
        let request = tonic::Request::new(cri::RunPodSandboxRequest {
            config: Some(pod_state.sandbox_config.clone()),
            runtime_handler: "".to_string(),
        });
        debug!("Sending request: {:?}", &request);
        let mut client = match pod_state.shared.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let response = match client.run_pod_sandbox(request).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                warn!(
                    "Error creating sandbox: {:?}. Remove existing sandbox and retry.",
                    e
                );
                stop_and_delete_pod_sandbox(&pod_state, pod.clone()).await?;
                let request = tonic::Request::new(cri::RunPodSandboxRequest {
                    config: Some(pod_state.sandbox_config.clone()),
                    runtime_handler: "".to_string(),
                });
                match client.run_pod_sandbox(request).await {
                    Ok(response) => response.into_inner(),
                    Err(e) => {
                        error!("Error making request: {:?}", &e);
                        anyhow::bail!(e);
                    }
                }
            }
        };
        info!("Started pod sandbox {}: {:?}", pod.name(), &response);
        let pod_sandbox_id = response.pod_sandbox_id;

        for container in pod.containers() {
            let image: String = container.image()?.unwrap().into();
            debug!("Creating container: {}", container.name());

            tokio::fs::create_dir_all(format!(
                "/var/log/pods/{}/{}/{}",
                pod.namespace(),
                pod.name(),
                container.name()
            ))
            .await?;

            let metadata = Some(cri::ContainerMetadata {
                name: container.name().to_string(),
                attempt: 0,
            });

            let image = Some(cri::ImageSpec {
                image: image.clone(),
            });

            let command = container.command().clone().unwrap_or_else(Vec::new);

            let args = container.args().clone().unwrap_or_else(Vec::new);

            let working_dir = container
                .working_dir()
                .cloned()
                .unwrap_or_else(|| "/".to_string());

            // TODO: Support value_from
            let envs = container
                .env()
                .clone()
                .unwrap_or_else(Vec::new)
                .into_iter()
                .filter_map(|env| match env.value {
                    Some(value) => Some(k8s_cri::v1alpha2::KeyValue {
                        key: env.name,
                        value,
                    }),
                    None => None,
                })
                .collect();

            // TODO
            let mounts = vec![];

            // TODO
            let devices = vec![];

            let labels = std::collections::BTreeMap::new();

            let annotations = std::collections::BTreeMap::new();

            let log_path = format!("{}/log", container.name());

            let linux = None;

            let config = Some(cri::ContainerConfig {
                metadata,
                image,
                command,
                args,
                working_dir,
                envs,
                mounts,
                devices,
                labels,
                annotations,
                log_path,
                stdin: false,
                stdin_once: false,
                tty: false,
                linux,
                windows: None,
            });

            let request = tonic::Request::new(cri::CreateContainerRequest {
                pod_sandbox_id: pod_sandbox_id.clone(),
                config,
                sandbox_config: Some(pod_state.sandbox_config.clone()),
            });
            debug!("Sending request: {:?}", &request);
            let response = match client.create_container(request).await {
                Ok(response) => response.into_inner(),
                Err(e) => {
                    error!("Error making request: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            debug!("Created container {}: {:?}", container.name(), &response);
            let container_id = response.container_id;

            debug!("Starting container: {}", container.name());
            let request = tonic::Request::new(cri::StartContainerRequest { container_id });
            debug!("Sending request: {:?}", &request);
            let response = match client.start_container(request).await {
                Ok(response) => response.into_inner(),
                Err(e) => {
                    error!("Error making request: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            info!("Started container {}: {:?}", container.name(), &response);
        }
        Ok(Transition::next(self, Running))
    }

    async fn json_status(
        &self,
        _pod_state: &mut PodState,
        _pod: &Pod,
    ) -> anyhow::Result<serde_json::Value> {
        make_status(Phase::Pending, "Starting")
    }
}

impl TransitionTo<Running> for Starting {}
