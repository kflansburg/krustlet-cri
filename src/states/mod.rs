use async_trait::async_trait;
use k8s_cri::v1alpha2 as cri;
use log::{debug, error, info};
use std::convert::TryFrom;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

mod error;
mod image_pull;
mod registered;
mod running;
mod starting;
mod terminated;

pub(crate) use registered::Registered;
pub(crate) use terminated::Terminated;

use crate::provider::{ContainerMap, PodMap};

#[derive(Clone)]
pub struct SharedPodState {
    pub pods: PodMap,
    pub containers: ContainerMap,
    pub socket_address: &'static str,
    pub kubeconfig: kube::Config,
}

impl SharedPodState {
    pub async fn client(
        &self,
    ) -> anyhow::Result<cri::runtime_service_client::RuntimeServiceClient<Channel>> {
        let path = self.socket_address;
        let channel = Endpoint::try_from("lttp://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| UnixStream::connect(path)))
            .await?;
        let client = cri::runtime_service_client::RuntimeServiceClient::new(channel);
        Ok(client)
    }

    pub async fn refresh_containers(&self) -> anyhow::Result<()> {
        debug!("Loading containers.");
        let request = tonic::Request::new(cri::ListContainersRequest { filter: None });
        debug!("Sending request: {:?}", &request);
        let mut client = match self.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let response = match client.list_containers(request).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };

        debug!("{:?}", &response);
        info!("Found {} containerss.", response.containers.len());

        let mut containers = self.containers.write().await;
        *containers = std::collections::HashMap::new();
        for container in response.containers {
            if let Some(meta) = container.metadata.clone() {
                containers.insert((container.pod_sandbox_id.clone(), meta.name), container);
            }
        }
        Ok(())
    }

    pub async fn refresh_pods(&self) -> anyhow::Result<()> {
        debug!("Refreshing Pods.");
        let request = tonic::Request::new(cri::ListPodSandboxRequest { filter: None });
        debug!("Sending request: {:?}", &request);
        let mut client = match self.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let response = match client.list_pod_sandbox(request).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };

        debug!("{:?}", &response);
        info!("Found {} pods.", response.items.len());

        let mut pods = self.pods.write().await;
        *pods = std::collections::HashMap::new();
        for pod in response.items {
            if let Some(meta) = pod.metadata.clone() {
                pods.insert((meta.namespace, meta.name), pod);
            }
        }
        Ok(())
    }
}

pub struct PodState {
    pub shared: SharedPodState,
    pub sandbox_config: cri::PodSandboxConfig,
}

impl PodState {
    pub fn pod_name(&self) -> String {
        self.sandbox_config.metadata.as_ref().unwrap().name.clone()
    }
    pub fn pod_namespace(&self) -> String {
        self.sandbox_config
            .metadata
            .as_ref()
            .unwrap()
            .namespace
            .clone()
    }
}

#[async_trait]
impl kubelet::state::AsyncDrop for PodState {
    async fn async_drop(self) {
        self.shared
            .pods
            .write()
            .await
            .remove(&(self.pod_namespace(), self.pod_name()));
    }
}
