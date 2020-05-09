mod provider;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = kubelet::config::Config::new_from_flags(env!("CARGO_PKG_VERSION"));

    let kubeconfig = kube::Config::infer().await?;

    // Initialize the logger
    env_logger::init();

    let provider = provider::Provider::new_from_socket_address(
        "/run/containerd/containerd.sock",
        kubeconfig.clone(),
    );
    let kubelet = kubelet::Kubelet::new(provider, kubeconfig, config);
    kubelet.start().await

    // println!("RESPONSE={:?}", response);
    // Ok(())
}
