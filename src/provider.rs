use async_trait::async_trait;
use k8s_cri::v1alpha2 as cri;
use log::{trace, debug, error, info, warn};
use std::convert::TryFrom;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;
use chrono::{Utc};

type Namespace = String;
type Name = String;
type PodId = String;

pub struct Provider {
    socket_address: &'static str,
    kubeconfig: kube::Config,
    pods: tokio::sync::RwLock<std::collections::HashMap<(Namespace, Name), PodId>>
}

impl Provider {
    pub fn new_from_socket_address(socket_address: &'static str, kubeconfig: kube::Config) -> Self {
        Provider { socket_address, kubeconfig, pods: tokio::sync::RwLock::new(std::collections::HashMap::new()) }
    }

    async fn load_pods(&self) -> anyhow::Result<()> {
        debug!("Loading running pods.");
        let request = tonic::Request::new(cri::ListPodSandboxRequest {
            filter: None,
        });
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
        info!("Found {} pods.", response.len());

        let mut pods = self.pods.write().await;
        *pods = std::collections::HashMap::new();
        for pod in response.items {
            if let Some(meta) = pod.metadata {
                pods.insert((meta.namespace, meta.name), pod.id);
            }
        }
        Ok(())
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

    async fn image_client(
        &self,
    ) -> anyhow::Result<cri::image_service_client::ImageServiceClient<Channel>> {
        let path = self.socket_address;
        let channel = Endpoint::try_from("lttp://[::]:50051")?
            .connect_with_connector(service_fn(move |_: Uri| UnixStream::connect(path)))
            .await?;

        let client = cri::image_service_client::ImageServiceClient::new(channel);
        Ok(client)
    }

}

const AMD64: &'static str = "amd64";

#[async_trait]
impl kubelet::Provider for Provider {
    const ARCH: &'static str = AMD64;

    async fn add(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        info!("ADD called for namespace {} pod {}", pod.namespace(), pod.name());

        debug!("Starting pod sandbox {}", pod.name());
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

        let log_directory = format!("/var/log/pods/{}/{}/", pod.namespace(), pod.name());

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

        let request = tonic::Request::new(cri::RunPodSandboxRequest {
            config: Some(sandbox_config.clone()),
            runtime_handler: "".to_string(),
        });
        debug!("Sending request: {:?}", &request);
        let mut client = match self.client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating client: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        let response = match client.run_pod_sandbox(request).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        info!("Started pod sandbox {}: {:?}", pod.name(), &response);
        let pod_sandbox_id = response.pod_sandbox_id;

        let mut status = kubelet::status::Status {
            message: None,
            container_statuses: std::collections::HashMap::new()
        };
        let mut image_client = match self.image_client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating image client: {:?}", &e);
                anyhow::bail!(e);
            }
        };

        for container in pod.containers() {
            // TODO Support image pull policy.
            let image = container.image.as_ref().unwrap().to_string();
            debug!("Pulling image: {}", &image);
            let request = tonic::Request::new(cri::PullImageRequest {
                image: Some(cri::ImageSpec { image: image.clone() }),
                // TODO support registry auth
                auth: None,
                sandbox_config: Some(sandbox_config.clone())
            });
            debug!("Sending request: {:?}", &request);
            let response = match image_client.pull_image(request).await {
                Ok(response) => response.into_inner(),
                Err(e) => {
                    error!("Error making request: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            info!("Pulled image: {:?}", response);

            debug!("Creating container: {}", &container.name);

            tokio::fs::create_dir_all(format!("/var/log/pods/{}/{}/{}", pod.namespace(), pod.name(), &container.name)).await?;

            let metadata = Some(cri::ContainerMetadata {
                name: container.name.clone(),
                attempt: 0
            });

            let image = Some(cri::ImageSpec { image: image.clone() });

            let command = container.command.clone().unwrap_or(vec![]);

            let args = container.args.clone().unwrap_or(vec![]);

            let working_dir = container.working_dir.clone().unwrap_or("/".to_string());

            // TODO
            let envs = vec![];

            // TODO
            let mounts = vec![];

            // TODO
            let devices = vec![];

            // TODO
            let labels = std::collections::HashMap::new();
    
            // TODO
            let annotations = std::collections::HashMap::new();

            let log_path = format!("{}/log", &container.name);

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
                windows: None
            });

            let request = tonic::Request::new(cri::CreateContainerRequest {
                pod_sandbox_id: pod_sandbox_id.clone(),
                config,
                sandbox_config: Some(sandbox_config.clone())
            });
            debug!("Sending request: {:?}", &request);
            let response = match client.create_container(request).await {
                Ok(response) => response.into_inner(),
                Err(e) => {
                    error!("Error making request: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            debug!("Created container {}: {:?}", &container.name, &response);
            let container_id = response.container_id;

            debug!("Starting container: {}", &container.name);
            let request = tonic::Request::new(cri::StartContainerRequest {
                container_id
            });
            debug!("Sending request: {:?}", &request);
            let response = match client.start_container(request).await {
                Ok(response) => response.into_inner(),
                Err(e) => {
                    error!("Error making request: {:?}", &e);
                    anyhow::bail!(e);
                }
            };
            info!("Started container {}: {:?}", &container.name, &response);
            status.container_statuses.insert(container.name.clone(), kubelet::status::ContainerStatus::Running { timestamp: Utc::now() } );
        }

        let client = kube::Client::new(self.kubeconfig.clone());
        pod.patch_status(client, status).await;
        debug!("Updated pod status.");

        Ok(())
    }

    async fn modify(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        info!(
            "MODIFY called for pod {} in namespace {}",
            pod.name(),
            pod.namespace()
        );
        trace!("Modified pod spec: {:#?}", pod.as_kube_pod());
        if let Some(timestamp) = pod.deletion_timestamp() {
            info!("Detected deletion: {}.", timestamp);
            self.load_pods().await?;

            match self.pods.read().await.get(&(pod.namespace().to_string(), pod.name().to_string())) {
                Some(id) => {
                    debug!("Stopping pod sandbox {}", pod.name());
                    let mut client = match self.client().await {
                        Ok(client) => client,
                        Err(e) => {
                            error!("Error creating client: {:?}", &e);
                            anyhow::bail!(e);
                        }
                    };
                    let request = tonic::Request::new(cri::StopPodSandboxRequest {
                        pod_sandbox_id: id.clone(),
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
                        pod_sandbox_id: id.clone(),
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

                    // Follow up with a delete when everything is stopped
                    let dp = kube::api::DeleteParams {
                        grace_period_seconds: Some(0),
                        ..Default::default()
                    };
                    let pod_client: kube::Api<k8s_openapi::api::core::v1::Pod> = kube::Api::namespaced(
                        kube::client::Client::new(self.kubeconfig.clone()),
                        pod.namespace(),
                    );
                    match pod_client.delete(pod.name(), &dp).await {
                        Ok(_) => Ok(()),
                        Err(e) => Err(e.into()),
                    }
                },
                None => {
                    warn!("Unkown pod.");
                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }

    async fn delete(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        info!("DELETE called for namespace {} pod {}", pod.namespace(), pod.name());
        self.pods.write().await.remove(&(pod.namespace().to_string(), pod.name().to_string()));
        Ok(())
    }

    async fn logs(
        &self,
        namespace: String,
        pod: String,
        container: String,
        _sender: kubelet::LogSender,
    ) -> anyhow::Result<()> {
        info!(
            "LOGS called for namespace {} pod {} container {}.",
            &namespace, &pod, &container
        );
        Ok(())
    }

    async fn exec(&self, pod: kubelet::Pod, command: String) -> anyhow::Result<Vec<String>> {
        info!("EXEC called for namespace {} pod {}: {} ", pod.namespace(), pod.name(), &command);
        Err(kubelet::provider::NotImplementedError.into())
    }
}
