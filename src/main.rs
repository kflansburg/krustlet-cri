mod provider;
use log::debug;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = kubelet::config::Config::new_from_flags(env!("CARGO_PKG_VERSION"));

    env_logger::init();

    debug!("Loading Kubeconfig.");
    let kubeconfig =
        kubelet::bootstrap(&config, &config.bootstrap_file, |s| println!("{}", s)).await?;

    debug!("Creating Provider.");
    let provider = provider::Provider::new_from_socket_address(
        "/run/containerd/containerd.sock",
        kubeconfig.clone(),
    );

    debug!("Creating Kubelet.");
    let kubelet = kubelet::Kubelet::new(provider, kubeconfig, config).await?;

    debug!("Running.");
    kubelet.start().await
}
