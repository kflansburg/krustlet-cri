use async_trait::async_trait;
use k8s_cri::v1alpha2 as cri;
use log::{debug, error, info};
use std::convert::TryFrom;
use tokio::net::UnixStream;
use tonic::transport::{Channel, Endpoint, Uri};
use tower::service_fn;

use super::{error::Error, starting::Starting, PodState};
use kubelet::state::prelude::*;

/// Kubelet is pulling container images.
#[derive(Default, Debug)]
pub struct ImagePull;

async fn make_image_client(
    path: &'static str,
) -> anyhow::Result<cri::image_service_client::ImageServiceClient<Channel>> {
    let channel = Endpoint::try_from("lttp://[::]:50051")?
        .connect_with_connector(service_fn(move |_: Uri| UnixStream::connect(path)))
        .await?;

    let client = cri::image_service_client::ImageServiceClient::new(channel);
    Ok(client)
}

async fn image_present(
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

#[async_trait]
impl State<PodState> for ImagePull {
    async fn next(
        self: Box<Self>,
        pod_state: &mut PodState,
        pod: &Pod,
    ) -> anyhow::Result<Transition<PodState>> {
        let mut image_client = match make_image_client(pod_state.shared.socket_address).await {
            Ok(client) => client,
            Err(e) => {
                let message = format!("Error creating image client: {:?}", &e);
                error!("{}", message);
                return Ok(Transition::next(self, Error { message }));
            }
        };

        for container in pod.containers() {
            let image: String = container.image()?.unwrap().into();
            let pull_policy = container.effective_pull_policy()?;
            info!("Image pull policy: {:?}", pull_policy);
            match pull_policy {
                kubelet::container::PullPolicy::Always => {
                    pull_image(&mut image_client, &image, &pod_state.sandbox_config).await?
                }
                kubelet::container::PullPolicy::IfNotPresent => {
                    if !image_present(&mut image_client, &image).await? {
                        info!("Image not present.");
                        pull_image(&mut image_client, &image, &pod_state.sandbox_config).await?
                    } else {
                        info!("Image present.");
                    }
                }
                kubelet::container::PullPolicy::Never => (),
            }
        }
        Ok(Transition::next(self, Starting))
    }

    async fn json_status(
        &self,
        _pod_state: &mut PodState,
        _pod: &Pod,
    ) -> anyhow::Result<serde_json::Value> {
        make_status(Phase::Pending, "ImagePull")
    }
}

impl TransitionTo<Error> for ImagePull {}
impl TransitionTo<Starting> for ImagePull {}
