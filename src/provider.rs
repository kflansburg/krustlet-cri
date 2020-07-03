use anyhow::bail;
use async_trait::async_trait;
use chrono::Utc;
use k8s_cri::v1alpha2 as cri;
use log::{debug, error, info, trace, warn};
use std::convert::TryFrom;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

type Namespace = String;
type Pod = String;
type Container = String;
type Id = String;

pub struct Provider {
    socket_address: &'static str,
    kubeconfig: kube::Config,
    pods: tokio::sync::RwLock<std::collections::HashMap<(Namespace, Pod), cri::PodSandbox>>,
    containers: tokio::sync::RwLock<std::collections::HashMap<(Id, Container), cri::Container>>,
}

impl Provider {
    pub fn new_from_socket_address(socket_address: &'static str, kubeconfig: kube::Config) -> Self {
        Provider {
            socket_address,
            kubeconfig,
            pods: tokio::sync::RwLock::new(std::collections::HashMap::new()),
            containers: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    async fn stop_and_delete_pod_sandbox(&self, pod: kubelet::pod::Pod) -> anyhow::Result<()> {
        match self
            .pods
            .read()
            .await
            .get(&(pod.namespace().to_string(), pod.name().to_string()))
        {
            Some(pod_sandbox) => {
                debug!("Stopping pod sandbox {}", pod.name());
                let mut client = match self.client().await {
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

    async fn pod_id(&self, namespace: &str, pod: &str) -> anyhow::Result<Id> {
        let key = (namespace.to_string(), pod.to_string());
        let has_pod = self.pods.read().await.contains_key(&key);
        if has_pod {
            Ok(self.pods.read().await.get(&key).unwrap().id.to_string())
        } else {
            self.get_pods().await?;
            let has_pod = self.pods.read().await.contains_key(&key);
            if has_pod {
                Ok(self.pods.read().await.get(&key).unwrap().id.to_string())
            } else {
                error!("Could not find namespace {} pod {}.", namespace, pod);
                bail!(kubelet::provider::ProviderError::PodNotFound {
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
        let has_container = self.containers.read().await.contains_key(&key);
        if has_container {
            Ok(self
                .containers
                .read()
                .await
                .get(&key)
                .unwrap()
                .id
                .to_string())
        } else {
            self.get_containers().await?;
            let has_container = self.containers.read().await.contains_key(&key);
            if has_container {
                Ok(self
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
                bail!(kubelet::provider::ProviderError::ContainerNotFound {
                    pod_name: pod.to_string(),
                    container_name: container.to_string(),
                });
            }
        }
    }

    async fn get_pods(&self) -> anyhow::Result<()> {
        debug!("Loading pods.");
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

    async fn get_containers(&self) -> anyhow::Result<()> {
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

    async fn describe_container(&self, container_id: Id) -> anyhow::Result<cri::ContainerStatus> {
        debug!("Describing container {}.", &container_id);
        let request = tonic::Request::new(cri::ContainerStatusRequest {
            container_id,
            verbose: false,
        });
        debug!("Sending request: {:?}", &request);
        let mut client = match self.client().await {
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
            bail!(
                "Container status response contained no status: {:?}",
                &response
            );
        }
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

    async fn image_present(
        &self,
        image_client: &mut cri::image_service_client::ImageServiceClient<Channel>,
        image: &str,
    ) -> anyhow::Result<bool> {
        let request = tonic::Request::new(cri::ImageStatusRequest {
            image: Some(cri::ImageSpec {
                image: image.to_string(),
            }),
            verbose: false,
        });
        debug!("Sending request: {:?}", &request);
        let response = match image_client.image_status(request).await {
            Ok(response) => response.into_inner(),
            Err(e) => {
                error!("Error making request: {:?}", &e);
                anyhow::bail!(e);
            }
        };
        Ok(response.image.is_some())
    }

    async fn pull_image(
        &self,
        image_client: &mut cri::image_service_client::ImageServiceClient<Channel>,
        image: &str,
        sandbox_config: &cri::PodSandboxConfig,
    ) -> anyhow::Result<()> {
        info!("Pulling image: {}", &image);
        let request = tonic::Request::new(cri::PullImageRequest {
            image: Some(cri::ImageSpec {
                image: image.to_string(),
            }),
            // TODO support registry auth
            auth: None,
            sandbox_config: Some(sandbox_config.clone()),
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
        Ok(())
    }
}

const AMD64: &str = "amd64";

#[async_trait]
impl kubelet::provider::Provider for Provider {
    const ARCH: &'static str = AMD64;

    async fn node(&self, builder: &mut kubelet::node::Builder) -> anyhow::Result<()> {
        let request = tonic::Request::new(cri::VersionRequest {
            version: "v1alpha2".to_string(),
        });
        debug!("Sending request: {:?}", &request);
        let mut client = match self.client().await {
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
            self.socket_address,
        );
        builder.set_architecture("amd64");
        Ok(())
    }

    async fn add(&self, pod: kubelet::pod::Pod) -> anyhow::Result<()> {
        info!(
            "ADD called for namespace {} pod {}",
            pod.namespace(),
            pod.name()
        );

        self.get_pods().await?;

        let pod_exists = {
            self.pods
                .read()
                .await
                .contains_key(&(pod.namespace().to_string(), pod.name().to_string()))
        };

        if pod_exists {
            self.stop_and_delete_pod_sandbox(pod.clone()).await?;
        }

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
                warn!(
                    "Error creating sandbox: {:?}. Remove existing sandbox and retry.",
                    e
                );
                self.stop_and_delete_pod_sandbox(pod.clone()).await?;
                let request = tonic::Request::new(cri::RunPodSandboxRequest {
                    config: Some(sandbox_config.clone()),
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

        let mut status = kubelet::pod::Status {
            message: None,
            container_statuses: std::collections::HashMap::new(),
        };
        let mut image_client = match self.image_client().await {
            Ok(client) => client,
            Err(e) => {
                error!("Error creating image client: {:?}", &e);
                anyhow::bail!(e);
            }
        };

        for container in pod.containers() {
            let image: String = container.image()?.unwrap().into();
            let pull_policy = container.image_pull_policy()?;
            info!("Image pull policy: {:?}", pull_policy);
            match pull_policy {
                kubelet::container::PullPolicy::Always => {
                    self.pull_image(&mut image_client, &image, &sandbox_config)
                        .await?
                }
                kubelet::container::PullPolicy::IfNotPresent => {
                    if !self.image_present(&mut image_client, &image).await? {
                        info!("Image not present.");
                        self.pull_image(&mut image_client, &image, &sandbox_config)
                            .await?
                    } else {
                        info!("Image present.");
                    }
                }
                kubelet::container::PullPolicy::Never => (),
            }

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
                .unwrap_or_else(|| vec![])
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
                sandbox_config: Some(sandbox_config.clone()),
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
            status.container_statuses.insert(
                container.name().to_string(),
                kubelet::container::Status::Running {
                    timestamp: Utc::now(),
                },
            );
        }

        let client = kube::Client::new(self.kubeconfig.clone());
        pod.patch_status(client, status).await;
        debug!("Updated pod status.");

        Ok(())
    }

    async fn modify(&self, pod: kubelet::pod::Pod) -> anyhow::Result<()> {
        info!(
            "MODIFY called for pod {} in namespace {}",
            pod.name(),
            pod.namespace()
        );
        trace!("Modified pod spec: {:#?}", pod.as_kube_pod());
        if let Some(timestamp) = pod.deletion_timestamp() {
            info!("Detected deletion: {}.", timestamp);
            self.get_pods().await?;
            self.stop_and_delete_pod_sandbox(pod.clone()).await?;
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
        } else {
            Ok(())
        }
    }

    async fn delete(&self, pod: kubelet::pod::Pod) -> anyhow::Result<()> {
        info!(
            "DELETE called for namespace {} pod {}",
            pod.namespace(),
            pod.name()
        );
        self.pods
            .write()
            .await
            .remove(&(pod.namespace().to_string(), pod.name().to_string()));
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
