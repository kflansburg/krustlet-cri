use k8s_cri::v1alpha2::{PodSandboxConfig, PodSandboxMetadata};
use std::convert::TryFrom;

#[derive(Debug)]
enum ConvertError {
    MissingField(String),
}

impl std::fmt::Display for ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConvertError::MissingField(ref s) => {
                write!(f, "Missing field during conversion: {}", s)
            }
        }
    }
}

impl std::error::Error for ConvertError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}

fn convert_metadata(
    meta: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta,
) -> anyhow::Result<PodSandboxMetadata> {
    let name = meta
        .name
        .ok_or_else(|| ConvertError::MissingField(format!("name")))?;
    let namespace = meta.namespace.unwrap_or_else(|| format!("default"));
    let attempt = meta.generation.unwrap_or_else(|| 0) as u32;
    let uid = meta
        .uid
        .ok_or_else(|| ConvertError::MissingField(format!("uid")))?;

    Ok(PodSandboxMetadata {
        name,
        namespace,
        uid,
        attempt,
    })
}

// impl TryFrom<k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta> for PodSandboxMetadata {
//     type Error = anyhow::Error;
//
//     fn try_from(meta: k8s_openapi::apimachinery::pkg::apis::meta::v1::ObjectMeta) -> anyhow::Result<PodSandboxMetadata> {
//         let name = meta.name.ok_or_else(|| ConvertError::MissingField(format!("name")))?;
//         let namespace = meta.namespace.unwrap_or_else(|| format!("default"));
//         let attempt = meta.namespace.unwrap_or_else(|| 0) as u32;
//         let uid = meta.uid.ok_or_else(|| ConvertError::MissingField(format!("uid")))?;
//
//         Ok(PodSandboxMetadata {
//             name,
//             namespace,
//             uid,
//             attempt
//         })
//     }
// }

// impl TryFrom<k8s_openapi::api::core::v1::Pod> for PodSandboxConfig {
//     type Error = anyhow::Error;
//
//     fn try_from(meta: k8s_openapi::api::core::v1::Pod) -> anyhow::Result<PodSandboxConfig> {
//         unimplemented!()
//     }
// }

// impl From<k8s_openapi::api::core::v1::Pod> for PodSandboxConfig {
//     fn from(pod: k8s_openapi::api::core::v1::Pod) -> Self {
//         let metadata = pod.metadata.map(|meta| {
//             PodSandboxMetadata {
//                 name: meta.name,
//                 namespace: meta.namespace,
//                 uid: meta.uid,
//                 attempt: meta.generation.map(|n| n as u32)
//             }
//         });
//
//         unimplemented!()
//         // crate::cri::PodSandboxConfig {
//         //     metadata: Some(krustlet_cri::cri::PodSandboxMetadata {
//         //         name:
//         //         namespace: ,
//         //         attempt: ,
//         //         uid: ,
//         //     }),
//         //     hostanme:
//         // }
//     }
// }
