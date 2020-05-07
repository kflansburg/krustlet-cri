use async_trait::async_trait;
use k8s_cri::v1alpha2 as cri;
use log::{debug, error, info, warn};
use std::convert::TryFrom;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

pub struct Provider {
    socket_address: &'static str,
}

impl Provider {
    pub fn new_from_socket_address(socket_address: &'static str) -> Self {
        Provider { socket_address }
    }

    async fn client(
        &self,
    ) -> anyhow::Result<cri::runtime_service_client::RuntimeServiceClient<Channel>> {
        let path = self.socket_address;
        let channel = Endpoint::try_from("lttp://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| UnixStream::connect(path)))
            .await?;

        let client = cri::runtime_service_client::RuntimeServiceClient::new(channel);
        Ok(client)
    }
}

const AMD64: &'static str = "amd64";

#[async_trait]
impl kubelet::Provider for Provider {
    const ARCH: &'static str = AMD64;

    async fn add(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        debug!("ADD called for pod {:?}", &pod);
        info!("Starting pod sandbox {}", pod.name());

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
            .unwrap_or("".to_string());

        // TODO
        let log_directory = "".to_string();

        // TODO
        let dns_config = Some(cri::DnsConfig {
            servers: vec![],
            searches: vec![],
            options: vec![],
        });

        // TODO
        let port_mappings = vec![];

        // TODO
        let labels = std::collections::HashMap::new();

        // TODO
        let annotations = std::collections::HashMap::new();

        let linux = None;

        let config = cri::PodSandboxConfig {
            metadata,
            hostname,
            log_directory,
            dns_config,
            port_mappings,
            labels,
            annotations,
            linux,
        };

        let request = tonic::Request::new(cri::RunPodSandboxRequest {
            config: Some(config),
            runtime_handler: "".to_string(),
        });
        info!("Sending request: {:?}", &request);
        let mut client = match self.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let response = match client.run_pod_sandbox(request).await {
            Ok(response) => response,
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        info!("Started pod sandbox {}: {:?}", pod.name(), response);

        Ok(())
    }

    async fn modify(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        debug!("MODIFY called for pod {:?}", &pod);
        warn!("MODIFY Unimplemented.");
        Ok(())
    }

    async fn delete(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        debug!("DELETE called for pod {:?}", &pod);

        let mut client = self.client().await?;

        info!("Stopping pod sandbox {}", pod.name());
        let mut client = match self.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let request = tonic::Request::new(cri::StopPodSandboxRequest {
            pod_sandbox_id: pod.name().to_string(),
        });
        info!("Sending request: {:?}", &request);
        let response = match client.stop_pod_sandbox(request).await {
            Ok(response) => response,
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        info!("Stopped pod sandbox {}: {:?}", pod.name(), response);

        info!("Removing pod sandbox {}", pod.name());
        let request = tonic::Request::new(cri::RemovePodSandboxRequest {
            pod_sandbox_id: pod.name().to_string(),
        });
        info!("Sending request: {:?}", &request);
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

    async fn logs(
        &self,
        namespace: String,
        pod: String,
        container: String,
        sender: kubelet::LogSender,
    ) -> anyhow::Result<()> {
        debug!(
            "Logs called for namespace {} pod {} container {}.",
            &namespace, &pod, &container
        );
        Ok(())
    }

    async fn exec(&self, pod: kubelet::Pod, command: String) -> anyhow::Result<Vec<String>> {
        debug!("EXEC called for command: {} pod {:?}", &command, &pod);
        Err(kubelet::provider::NotImplementedError.into())
    }
}
