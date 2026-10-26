//! Discovery de tipos de recurso y su agrupación en las categorías de la barra lateral.

use kube::Client;
use kube::discovery::{Discovery, Scope};
use oculus_protocol::{Category, ResourceKind};

/// Usa el discovery agregado (una sola petición, Kubernetes ≥ 1.30) y, si el
/// servidor no lo soporta, el clásico de una petición por grupo.
pub async fn discover(client: &Client) -> Result<Vec<ResourceKind>, kube::Error> {
    let discovery = match Discovery::new(client.clone()).run_aggregated().await {
        Ok(d) => d,
        Err(err) => {
            tracing::debug!(%err, "sin discovery agregado; se usa el clásico");
            Discovery::new(client.clone()).run().await?
        }
    };

    let mut kinds: Vec<ResourceKind> = discovery
        .groups()
        .flat_map(|group| group.recommended_resources())
        .map(|(ar, caps)| ResourceKind {
            category: categorize(&ar.group, &ar.kind),
            namespaced: caps.scope == Scope::Namespaced,
            verbs: caps.operations.clone(),
            group: ar.group,
            version: ar.version,
            kind: ar.kind,
            plural: ar.plural,
        })
        .collect();
    kinds.sort_by(|a, b| (a.category, &a.kind, &a.group).cmp(&(b.category, &b.kind, &b.group)));
    Ok(kinds)
}

pub fn categorize(group: &str, kind: &str) -> Category {
    use Category::*;
    match group {
        "" => match kind {
            "Pod" | "ReplicationController" | "PodTemplate" => Workloads,
            "ConfigMap" | "Secret" | "ResourceQuota" | "LimitRange" => Config,
            "Service" | "Endpoints" => Network,
            "PersistentVolume" | "PersistentVolumeClaim" => Storage,
            "ServiceAccount" => Access,
            _ => Cluster,
        },
        "apps" | "batch" => Workloads,
        "autoscaling" | "policy" | "admissionregistration.k8s.io" => Config,
        "networking.k8s.io" | "discovery.k8s.io" | "gateway.networking.k8s.io" => Network,
        "storage.k8s.io" | "snapshot.storage.k8s.io" => Storage,
        "rbac.authorization.k8s.io" | "authentication.k8s.io" | "authorization.k8s.io" => Access,
        g if g.ends_with(".k8s.io") => Cluster,
        _ => Custom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agrupa_los_tipos_nativos_y_deja_los_crds_en_custom() {
        assert_eq!(categorize("", "Pod"), Category::Workloads);
        assert_eq!(categorize("apps", "Deployment"), Category::Workloads);
        assert_eq!(categorize("", "Secret"), Category::Config);
        assert_eq!(categorize("networking.k8s.io", "Ingress"), Category::Network);
        assert_eq!(categorize("", "PersistentVolumeClaim"), Category::Storage);
        assert_eq!(categorize("rbac.authorization.k8s.io", "Role"), Category::Access);
        assert_eq!(categorize("", "Namespace"), Category::Cluster);
        assert_eq!(categorize("apiextensions.k8s.io", "CustomResourceDefinition"), Category::Cluster);
        assert_eq!(categorize("cert-manager.io", "Certificate"), Category::Custom);
    }
}
