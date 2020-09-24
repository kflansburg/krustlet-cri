use async_trait::async_trait;
use k8s_cri::v1alpha2 as cri;
use log::{debug, error, info};
use std::sync::Arc;

use crate::states::{PodState, Registered, SharedPodState, Terminated};

type Namespace = String;
type Pod = String;
type Container = String;
type Id = String;

pub(crate) type PodMap =
    Arc<tokio::sync::RwLock<std::collections::HashMap<(Namespace, Pod), cri::PodSandbox>>>;
pub(crate) type ContainerMap =
    Arc<tokio::sync::RwLock<std::collections::HashMap<(Id, Container), cri::Container>>>;

pub struct Provider {
    shared: SharedPodState,
}

impl Provider {
    pub fn new_from_socket_address(socket_address: &'static str, kubeconfig: kube::Config) -> Self {
        Provider {
            shared: SharedPodState {
                socket_address,
                kubeconfig,
                pods: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
                containers: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            },
        }
    }

    async fn pod_id(&self, namespace: &str, pod: &str) -> anyhow::Result<Id> {
        let key = (namespace.to_string(), pod.to_string());
        let has_pod = self.shared.pods.read().await.contains_key(&key);
        if has_pod {
            Ok(self
                .shared
                .pods
                .read()
                .await
                .get(&key)
                .unwrap()
                .id
                .to_string())
        } else {
            self.shared.refresh_pods().await?;
            let has_pod = self.shared.pods.read().await.contains_key(&key);
            if has_pod {
                Ok(self
                    .shared
                    .pods
                    .read()
                    .await
                    .get(&key)
                    .unwrap()
                    .id
                    .to_string())
            } else {
                error!("Could not find namespace {} pod {}.", namespace, pod);
                anyhow::bail!(kubelet::provider::ProviderError::PodNotFound {
                    pod_name: pod.to_string(),
                });
            }
        }
    }

    async fn container_id(
        &self,
        namespace: &str,
        pod: &str,
        container: &str,
    ) -> anyhow::Result<Id> {
        let pod_id = self.pod_id(namespace, pod).await?;
        let key = (pod_id, container.to_string());
        let has_container = self.shared.containers.read().await.contains_key(&key);
        if has_container {
            Ok(self
                .shared
                .containers
                .read()
                .await
                .get(&key)
                .unwrap()
                .id
                .to_string())
        } else {
            self.shared.refresh_containers().await?;
            let has_container = self.shared.containers.read().await.contains_key(&key);
            if has_container {
                Ok(self
                    .shared
                    .containers
                    .read()
                    .await
                    .get(&key)
                    .unwrap()
                    .id
                    .to_string())
            } else {
                error!(
                    "Could not find namespace {} pod {} container {}.",
                    namespace, pod, container
                );
                Err(anyhow::anyhow!(
                    kubelet::provider::ProviderError::ContainerNotFound {
                        pod_name: pod.to_string(),
                        container_name: container.to_string(),
                    }
                ))
            }
        }
    }

    async fn describe_container(&self, container_id: Id) -> anyhow::Result<cri::ContainerStatus> {
        debug!("Describing container {}.", &container_id);
        let request = tonic::Request::new(cri::ContainerStatusRequest {
            container_id,
            verbose: false,
        });
        debug!("Sending request: {:?}", &request);
        let mut client = match self.shared.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let response = match client.container_status(request).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        debug!("{:?}", &response);

        if let Some(status) = response.status {
            Ok(status)
        } else {
            Err(anyhow::anyhow!(
                "Container status response contained no status: {:?}",
                &response
            ))
        }
    }
}

const AMD64: &str = "amd64";

#[async_trait]
impl kubelet::provider::Provider for Provider {
    type PodState = PodState;

    type InitialState = Registered;
    type TerminatedState = Terminated;

    const ARCH: &'static str = AMD64;

    async fn initialize_pod_state(
        &self,
        pod: &kubelet::pod::Pod,
    ) -> anyhow::Result<Self::PodState> {
        let metadata = Some(cri::PodSandboxMetadata {
            name: pod.name().to_string(),
            namespace: pod.namespace().to_string(),
            uid: "".to_string(),
            attempt: 0,
        });

        let hostname = pod
            .as_kube_pod()
            .spec
            .clone()
            .unwrap_or_default()
            .hostname
            .unwrap_or_else(|| "".to_string());

        let log_directory = format!("/var/log/pods/{}/{}/", pod.namespace(), pod.name());

        // TODO
        let dns_config = Some(cri::DnsConfig {
            servers: vec![],
            searches: vec![],
            options: vec![],
        });

        let port_mappings = vec![];

        let labels = pod.labels().clone();

        let annotations = pod.annotations().clone();

        let linux = None;

        let sandbox_config = cri::PodSandboxConfig {
            metadata,
            hostname,
            log_directory,
            dns_config,
            port_mappings,
            labels,
            annotations,
            linux,
        };

        Ok(PodState {
            shared: self.shared.clone(),
            sandbox_config,
        })
    }

    async fn node(&self, builder: &mut kubelet::node::Builder) -> anyhow::Result<()> {
        let request = tonic::Request::new(cri::VersionRequest {
            version: "v1alpha2".to_string(),
        });
        debug!("Sending request: {:?}", &request);
        let mut client = match self.shared.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let response = match client.version(request).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        info!("Found container runtime: {:?}", &response);

        builder.set_container_runtime_version(&format!(
            "{}://{}",
            &response.runtime_name, &response.runtime_version
        ));
        builder.add_annotation(
            "kubeadm.alpha.kubernetes.io/cri-socket",
            self.shared.socket_address,
        );
        builder.set_architecture("amd64");
        Ok(())
    }

    async fn logs(
        &self,
        namespace: String,
        pod: String,
        container: String,
        sender: kubelet::log::Sender,
    ) -> anyhow::Result<()> {
        info!(
            "LOGS called for namespace {} pod {} container {}.",
            &namespace, &pod, &container
        );
        let container_id = self.container_id(&namespace, &pod, &container).await?;
        let status = self.describe_container(container_id).await?;
        let handle = tokio::fs::File::open(status.log_path).await?;
        tokio::spawn(kubelet::log::stream(handle, sender));
        Ok(())
    }

    async fn exec(&self, pod: kubelet::pod::Pod, command: String) -> anyhow::Result<Vec<String>> {
        info!(
            "EXEC called for namespace {} pod {}: {} ",
            pod.namespace(),
            pod.name(),
            &command
        );
        Err(kubelet::provider::NotImplementedError.into())
    }
}
