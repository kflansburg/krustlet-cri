FROM rust:1.44 as BUILD

# Build depends
RUN USER=root cargo new --bin app
WORKDIR /app
COPY Cargo.toml .
RUN cargo build --release

# Build KrustletCRI
RUN rm src/*.rs
RUN rm ./target/release/deps/krustlet_cri*
COPY src/ src/
RUN cargo build --release

FROM debian:stable as RUN

RUN apt update && apt install -y wget iptables && rm -rf /var/lib/apt/lists/*

WORKDIR /

# Install ContainerD
ENV CONTAINERD_VERSION 1.3.4
RUN wget -q https://github.com/containerd/containerd/releases/download/v$CONTAINERD_VERSION/containerd-$CONTAINERD_VERSION.linux-amd64.tar.gz && \
    tar -xf containerd-$CONTAINERD_VERSION.linux-amd64.tar.gz && \
    rm containerd-$CONTAINERD_VERSION.linux-amd64.tar.gz

# Install `crictl`. Useful for debug.
ENV CRICTL_VERSION 1.18.0
RUN wget -q https://github.com/kubernetes-sigs/cri-tools/releases/download/v$CRICTL_VERSION/crictl-v$CRICTL_VERSION-linux-amd64.tar.gz && \
    tar -xf crictl-v$CRICTL_VERSION-linux-amd64.tar.gz && \
    rm crictl-v$CRICTL_VERSION-linux-amd64.tar.gz && \
    mv crictl bin/

# Install Kubectl
ENV KUBECTL_VERSION 1.18.0
RUN wget -q https://storage.googleapis.com/kubernetes-release/release/v$KUBECTL_VERSION/bin/linux/amd64/kubectl -O /bin/kubectl && chmod +x /bin/kubectl

# Install Krustlet TLS Bootstrap Script
RUN wget -q https://raw.githubusercontent.com/deislabs/krustlet/master/docs/howto/assets/bootstrap.sh -O /bin/bootstrap-tls && chmod +x /bin/bootstrap-tls

# Configure Basic CNI
RUN wget -q https://github.com/containernetworking/plugins/releases/download/v0.8.2/cni-plugins-linux-amd64-v0.8.2.tgz && \
    mkdir -p /etc/cni/net.d /opt/cni/bin && \
    tar -xf cni-plugins-linux-amd64-v0.8.2.tgz -C /opt/cni/bin/ && \
    rm cni-plugins-linux-amd64-v0.8.2.tgz

# Install RUNC
RUN wget -q https://github.com/opencontainers/runc/releases/download/v1.0.0-rc8/runc.amd64 && \
    mv runc.amd64 /bin/runc && \
    chmod +x /bin/runc

ADD demo/etc/cni/10-bridge.conf /etc/cni/net.d/10-bridge.conf
ADD demo/etc/cni/99-loopback.conf /etc/cni/net.d/99-loopback.conf
ADD demo/etc/containerd/config.toml /etc/containerd/config.toml

ENV CONTAINER_RUNTIME_ENDPOINT unix:///run/containerd/containerd.sock
ENV KUBECONFIG /root/.krustlet/config/kubeconfig

COPY --from=BUILD /app/target/release/krustlet-cri bin/krustlet-cri

COPY demo/entrypoint /bin/

ENTRYPOINT ["entrypoint"]
