use async_trait::async_trait;
use log::{debug, info, warn, error};

mod cri {
    tonic::include_proto!("runtime");
}

struct Provider;

const AMD64: &'static str = "amd64";

#[async_trait]
impl kubelet::Provider for Provider {
    const ARCH: &'static str = AMD64;

    async fn add(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        debug!("ADD called for pod {:?}", &pod);
        unimplemented!()
    }

    async fn modify(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        debug!("MODIFY called for pod {:?}", &pod);
        unimplemented!()
    }

    async fn delete(&self, pod: kubelet::Pod) -> anyhow::Result<()> {
        debug!("DELETE called for pod {:?}", &pod);
        unimplemented!()
    }

    async fn logs(
        &self,
        namespace: String,
        pod: String,
        container: String,
        sender: kubelet::LogSender,
        tail: Option<usize>,
        follow: bool,
    ) -> anyhow::Result<()> {
        debug!("Logs called for namespace {} pod {} container {} tail {:?} follow {}.", &namespace, &pod, &container, &tail, follow);
        unimplemented!()
    }

    async fn exec(&self, pod: kubelet::Pod, command: String) -> anyhow::Result<Vec<String>> {
        debug!("EXEC called for command: {} pod {:?}", &command, &pod);
        Err(kubelet::provider::NotImplementedError.into())
    }
}



#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = kubelet::config::Config::new_from_flags(env!("CARGO_PKG_VERSION"));

    let kubeconfig = kube::Config::infer().await?;

    // Initialize the logger
    env_logger::init();

    let provider = Provider;
    let kubelet = kubelet::Kubelet::new(provider, kubeconfig, config);
    kubelet.start().await
}
