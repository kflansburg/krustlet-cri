# krustlet-cri

The goal of this project is to build a fully-featured Kubelet in Rust by leveraging the [Krustlet Project from Deis Labs](https://github.com/deislabs/krustlet). 

* Fully `async` Rust to maximize performance.
* No `panics` and leverage Rust error handling for reliability.
* Use [CNI](https://github.com/containernetworking/cni/blob/master/SPEC.md#network-configuration) and [CRI](https://kubernetes.io/blog/2016/12/container-runtime-interface-cri-in-kubernetes/) exclusively to simplify development while maximizing support for existing and future container runtimes and network providers.

# What Works
* Node registration.
* Basic pod create and delete. 
* Container logs.
* Tested with `containerd`.

# Try It Out

This example uses Kind to demonstrate KrustletCRI. KrustletCRI will run in a privileged Docker container.

1. If you do not already have a Kind cluster running:

```
kind cluster create
```

2. Ensure that your `kubectl` is configured to use this Kind cluster by default, as it will be used for TLS Bootstrapping.
This should show the Kubernetes master for the Kind cluster:

```
kubectl cluster-info
```

3. Build the KrustletCRI image.

```
docker build -t krustlet-cri -f demo/Dockerfile .
```

4. Run KrustletCRI.

This setup will cache KrustletCRI credentials to a directory mounted from the host, create this directory:

```
mkdir .krustlet
```

This will:
* Launch and background `containerd`.
* Bootstrap Kubelet TLS certificates and configure them with the Kind cluster. (This can take a while the first time.)
* Launch KrustletCRI and follow log output.

```
docker run -it --privileged -p 3000:3000 -v $(pwd)/.krustlet:/root/.krustlet -v $HOME/.kube:/mnt/kube --network host --hostname krustlet-cri krustlet-cri
```


Once TLS bootstrapping had begun, you will need to approve the KrustletCRI certificate, in another shell:

```
kubectl certificate approve krustlet-cri-tls
```

6. Verify `krustlet-cri` has joined the node poll.

```
kubectl get nodes
```

7. Finally, schedule a Pod on KrustletCRI.

```
kubectl apply -f demo/hello.yaml
kubectl logs -f hello
```
