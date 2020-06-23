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

TODO - I would like to try to build an example using Docker in Docker for the quickest onboarding. 

# Incomplete Roadmap
 - [ ] Pod Networking / CNI
 - [ ] Annotations and Labels
 - [ ] Port Mappings
 - [ ] Image registry and pull policy.
 - [ ] Pod patches / updates
 - [ ] Environment variables
 - [ ] Mounts
 - [ ] Container `exec`
 
